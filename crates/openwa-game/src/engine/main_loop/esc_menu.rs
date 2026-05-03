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
//!  - **1** — open / mouse-driven cursor + LMB activation. Handled by
//!    [`tick_open`] (Rust port of `EscMenu_TickState1`).
//!  - **2** — confirm dialog (Yes/No) covering the original menu. Handled
//!    by [`tick_confirm`] (Rust port of `EscMenu_TickState2`).
//!
//! [`MenuPanel`]: crate::engine::menu_panel::MenuPanel
//! [`DisplayBitGrid`]: crate::bitgrid::DisplayBitGrid

use core::ffi::c_char;

use openwa_core::fixed::Fixed;

use crate::address::va;
use crate::audio::known_sound_id::KnownSoundId;
use crate::audio::sound_id::SoundId;
use crate::audio::sound_ops::dispatch_global_sound;
use crate::bitgrid::DisplayBitGrid;
use crate::engine::game_session::get_game_session;
use crate::engine::menu_panel::{
    ActivateOutcome, MenuPanel, activate_at_cursor, append_item_impl,
    center_cursor_on_first_kind_zero, set_cursor_at,
};
use crate::engine::runtime::GameRuntime;
use crate::engine::team_arena::TeamArena;
use crate::engine::world::GameWorld;
use crate::entity::WorldRootEntity;
use crate::input::keyboard::KeyboardAction;
use crate::input::mouse::MouseInput;
use crate::rebase::rb;
use crate::render::display::font::TextMeasurement;
use crate::render::display::gfx::DisplayGfx;
use crate::render::display::vtable::{draw_text_on_bitmap, measure_text};
use crate::render::sprite::sprite_op::SpriteOp;
use crate::wa::string_resource::res;

// ─── Bridged WA addresses ──────────────────────────────────────────────────

static mut APPLY_VOLUME_SETTINGS_ADDR: u32 = 0;
static mut BEGIN_ROUND_END_ADDR: u32 = 0;
static mut BEGIN_NETWORK_GAME_END_ADDR: u32 = 0;
static mut MENU_PANEL_RENDER_ADDR: u32 = 0;

// `GameRuntime::ApplyVolumeSettings` (0x00534B40) — usercall(EAX=this),
// plain RET. Copies [`GameRuntime::sound_volume`] (the slider's live value)
// into [`GameRuntime::ui_volume`], then pushes the scaled value to DSSound
// and Music via vtable calls. Bridged because typed wrappers for those two
// vtable slots aren't yet defined.
const APPLY_VOLUME_SETTINGS_VA: u32 = 0x00534B40;

/// Initialize the ESC-menu bridge addresses. Called from
/// `dispatch_frame::init_dispatch_addrs` at DLL load.
pub unsafe fn init_addrs() {
    unsafe {
        APPLY_VOLUME_SETTINGS_ADDR = rb(APPLY_VOLUME_SETTINGS_VA);
        BEGIN_ROUND_END_ADDR = rb(va::GAME_RUNTIME_BEGIN_ROUND_END);
        BEGIN_NETWORK_GAME_END_ADDR = rb(va::GAME_RUNTIME_BEGIN_NETWORK_GAME_END);
        MENU_PANEL_RENDER_ADDR = rb(va::MENU_PANEL_RENDER);
    }
}

// ─── Bridges (still WA-side) ───────────────────────────────────────────────

/// Bridge for `GameRuntime::ApplyVolumeSettings` (0x00534B40). Usercall
/// EAX=this, plain RET. Reads [`GameRuntime::sound_volume`] (the live
/// slider value) → writes [`GameRuntime::ui_volume`] → pushes the
/// effective value (clamped by the game-end fade and the engine-suspended
/// flag) to `DSSound::SetMasterVolume` and `Music::SetVolume`.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_apply_volume_settings(_this: *mut GameRuntime) {
    core::arch::naked_asm!(
        "mov eax, [esp+4]",
        "call [{addr}]",
        "ret 4",
        addr = sym APPLY_VOLUME_SETTINGS_ADDR,
    );
}

/// Bridge for `GameRuntime::BeginRoundEnd` (0x00536550). Usercall
/// `(EAX=this, [ESP+4]=skip_frame_delay)`, RET 0x4. Transitions runtime
/// to `game_state = ROUND_ENDING` (4), zeroes the game-end fade fields,
/// optionally clears the frame-delay counter (-1) when `skip_frame_delay
/// != 0`, and broadcasts msg `0x75` through `world_root.handle_message`
/// when `world.game_info[+0xD778] > 0x4C`. Tail-call shape: pop ret-addr
/// + the `this` arg, push ret-addr back, jmp to target. Target's RET 0x4
/// cleans `skip_frame_delay`.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_begin_round_end(_this: *mut GameRuntime, _skip_frame_delay: u32) {
    core::arch::naked_asm!(
        "pop ecx",
        "pop eax",
        "push ecx",
        "jmp dword ptr [{addr}]",
        addr = sym BEGIN_ROUND_END_ADDR,
    );
}

/// Bridge for `GameRuntime::BeginNetworkGameEnd` (0x00536270). Usercall
/// `(EAX=this)`, plain RET. Network-mode round-end entry: writes initial
/// peer-score sentinels (1000) into `runtime` per active peer, transitions
/// `game_state = NETWORK_END_AWAIT_PEERS` (3), enqueues a message in the
/// network ring buffer, and forwards through `net_session.handle_message`.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_begin_network_game_end(_this: *mut GameRuntime) {
    core::arch::naked_asm!(
        "pop ecx",
        "pop eax",
        "push ecx",
        "jmp dword ptr [{addr}]",
        addr = sym BEGIN_NETWORK_GAME_END_ADDR,
    );
}

/// Bridge for `MenuPanel::Render` (0x00540B00). Usercall(EDI = panel),
/// plain RET, returns the panel's [`DisplayBitGrid`] canvas in EAX.
/// Saves/restores EDI per the C callee-save ABI; the WA target itself
/// preserves it.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_menu_panel_render(_panel: *mut MenuPanel) -> *mut DisplayBitGrid {
    core::arch::naked_asm!(
        "push edi",
        "mov edi, dword ptr [esp+8]",
        "call dword ptr [{addr}]",
        "pop edi",
        "ret 4",
        addr = sym MENU_PANEL_RENDER_ADDR,
    );
}

// ─── UI sound IDs ──────────────────────────────────────────────────────────
//
// Both ESC-menu UI sounds set the "raw volume" flag (bit 17, 0x20000) so
// the slider's value is heard at exactly the chosen level — without master
// volume re-scaling — giving immediate feedback while the user drags. See
// [`SoundId::is_raw_volume`].

/// "Click" / "select" UI sound — raw_volume + [`KnownSoundId::CursorSelect`].
const UI_CLICK_SOUND: SoundId = SoundId::from_known(KnownSoundId::CursorSelect).with_raw_volume();
/// "Miss" / "rejected" UI sound — raw_volume + [`KnownSoundId::WarningBeep`].
const UI_MISS_SOUND: SoundId = SoundId::from_known(KnownSoundId::WarningBeep).with_raw_volume();

use crate::wa::localized_template::resolve as bridge_token_lookup;
use crate::wa::sprintf_rotating::sprintf_3 as bridge_sprintf_rotating_3;

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
        let entity = (*runtime).world_root;
        WorldRootEntity::hud_data_query_raw(entity, 0x7D3, 0x394, buf.as_mut_ptr() as *mut u8);

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

/// Predicate: are we at slot 0 of `world.team_index_maps[map_idx]`'s
/// active list? Returns `true` only when `runtime_handle` is found AND
/// it's the first element (slot 0). Used by [`tick_open`] to gate the
/// menu on input focus — the ESC menu only accepts mouse/keyboard input
/// while the player owns slots 0 of both map[0] (frame-input subscribers)
/// and map[2] (UI-input subscribers).
unsafe fn handle_at_input_focus(world: *const GameWorld, map_idx: usize, handle: i32) -> bool {
    unsafe {
        let map = &(*world).team_index_maps[map_idx];
        let count = map.active_count as usize;
        if count == 0 {
            return false;
        }
        let target = handle as i16;
        // WA's exact loop: the first match wins. If found at index != 0,
        // we don't own focus (some other subscriber is ahead of us).
        for (i, &slot) in map.active_list.iter().take(count).enumerate() {
            if slot as i32 == handle as i32 {
                return i == 0;
            }
            // (target var silences "unused if no match" — compiler can prove it.)
            let _ = target;
        }
        false
    }
}

/// Rust port of `GameRuntime::EscMenu_TickState1` (0x00535B10) — the
/// per-frame mouse-driven input handler while the menu is open
/// (`esc_menu_state == 1`). Despite the "TickState1" Ghidra name, this
/// function only handles mouse input + Escape: there is no arrow-key
/// navigation in the ESC menu — the cursor IS the screen mouse cursor.
///
/// Sequence:
/// 1. Bail early unless we own input focus on both `team_index_maps[0]`
///    and `team_index_maps[2]` (handles
///    [`runtime._field_438`](GameRuntime::_field_438) /
///    [`_field_43c`](GameRuntime::_field_43c) at slot 0).
/// 2. If [`KeyboardAction::Escape`] is just-pressed
///    ([`is_active2`](KeyboardAction::is_active2)) → close the menu
///    (`esc_menu_state = 0`).
/// 3. Poll mouse via
///    [`MouseInput::consume_delta_and_buttons`](crate::input::mouse::MouseInputVtable::consume_delta_and_buttons)
///    → cursor delta + debounced LMB press.
/// 4. Move cursor: [`set_cursor_at`]`(panel, cursor + delta)`.
/// 5. If LMB not pressed → release the slider drag lock and return.
/// 6. Otherwise [`activate_at_cursor`]:
///    * [`Slider`](ActivateOutcome::Slider) → if the volume slider value
///      changed, [`bridge_apply_volume_settings`] + click sound.
///    * [`Miss`](ActivateOutcome::Miss) → ack mouse latch + miss sound.
///    * [`Button`](ActivateOutcome::Button) `kind == 3` (Minimize Game)
///      → set `g_GameSession.minimize_request = 1`; close menu only when
///      online (`world.net_session != null`) — offline keeps the menu
///      open behind the minimised window.
///    * Other button kinds (Force-SD / Draw / Quit) → click sound +
///      [`open_confirm_dialog`] (transitions to state 2).
pub unsafe fn tick_open(runtime: *mut GameRuntime) {
    unsafe {
        let world = (*runtime).world;

        // Step 1: input-focus gate.
        if !handle_at_input_focus(world, 0, (*runtime)._field_438) {
            return;
        }
        if !handle_at_input_focus(world, 2, (*runtime)._field_43c) {
            return;
        }

        // Step 2: Escape closes the menu.
        let keyboard = (*world).keyboard;
        if KeyboardAction::Escape.is_active2(keyboard) {
            (*runtime).esc_menu_state = 0;
            return;
        }

        // Step 3: poll mouse — read deltas + button latch, then zero the
        // delta accumulator so the next frame's read measures fresh
        // movement only. (`Mouse__ConsumeDeltaAndButtons` reads but does
        // not clear; vanilla TickState1 always pairs it with a slot-3
        // `clear_deltas` call in this exact sequence.)
        let mouse_input = (*world).mouse_input;
        let mut dx: i32 = 0;
        let mut dy: i32 = 0;
        let mut buttons: u32 = 0;
        MouseInput::consume_delta_and_buttons_raw(mouse_input, &mut dx, &mut dy, &mut buttons);
        MouseInput::clear_deltas_raw(mouse_input);

        // Step 4: move cursor.
        let panel = (*runtime).menu_panel_a;
        let new_x = (*panel).cursor_x + dx;
        let new_y = (*panel).cursor_y + dy;
        set_cursor_at(panel, new_x, new_y);

        // Step 5: LMB not pressed → release drag lock + return.
        if buttons & 1 == 0 {
            (*panel).slider_lock = 0;
            return;
        }

        // Step 6: dispatch by activation outcome.
        match activate_at_cursor(panel) {
            ActivateOutcome::Slider(_) => {
                // Apply the slider's new sound volume only if it differs
                // from the last-applied snapshot (`ui_volume`).
                if (*runtime).sound_volume != (*runtime).ui_volume {
                    bridge_apply_volume_settings(runtime);
                    dispatch_global_sound(
                        runtime,
                        UI_CLICK_SOUND,
                        8,
                        Fixed::ONE,
                        (*runtime).ui_volume,
                    );
                }
            }
            ActivateOutcome::Miss => {
                // Re-arm LMB+RMB latch bits so the next click registers.
                MouseInput::ack_button_mask_raw(mouse_input, 3);
                dispatch_global_sound(runtime, UI_MISS_SOUND, 8, Fixed::ONE, (*runtime).ui_volume);
            }
            ActivateOutcome::Button(kind) => {
                MouseInput::ack_button_mask_raw(mouse_input, 3);
                if kind == 3 {
                    // Minimize Game: post the SC_MINIMIZE request.
                    (*get_game_session()).minimize_request = 1;
                    // Online: close menu (don't block input under the
                    // minimised window). Offline: leave it open.
                    if !(*world).net_session.is_null() {
                        (*runtime).esc_menu_state = 0;
                    }
                } else {
                    // Force-SD / Draw / Quit: confirm via state 2.
                    dispatch_global_sound(
                        runtime,
                        UI_CLICK_SOUND,
                        8,
                        Fixed::ONE,
                        (*runtime).ui_volume,
                    );
                    open_confirm_dialog(runtime);
                }
            }
        }
    }
}

/// Rust port of `GameRuntime::EscMenu_TickState2` (0x00535FC0) — the
/// per-frame input handler while the confirm dialog is up
/// (`esc_menu_state == 2`). The dialog itself was built into
/// [`menu_panel_b`](GameRuntime::menu_panel_b) by
/// `OpenEscMenuConfirmDialog`; it's a Yes (kind=2) / No (kind=1) overlay
/// covering the original menu.
///
/// Sequence (largely a sibling of [`tick_open`]):
/// 1. Same input-focus gate as state 1 (slot 0 of `team_index_maps[0]`
///    and `[2]`).
/// 2. If [`KeyboardAction::Escape`] is just-pressed → bounce back to
///    state 1 (cancel the confirm).
/// 3. Mouse poll on `world.mouse_input`: `consume_delta_and_buttons` +
///    **unconditional** `ack_button_mask(3)` + `clear_deltas`. Unlike
///    state 1, the LMB+RMB latch is acked every frame here — there's no
///    drag-lock to preserve in the confirm dialog.
/// 4. Move the **panel-B** cursor by the delta: [`set_cursor_at`]`(panel_b,
///    cursor + delta)`.
/// 5. Bail unless LMB is pressed.
/// 6. [`activate_at_cursor`] on `panel_b`:
///    * [`Miss`](ActivateOutcome::Miss) → miss sound; return (state stays 2).
///    * Hit → click sound. If the resulting kind is `1` (the No button)
///      → bounce back to state 1.
/// 7. Otherwise re-run [`activate_at_cursor`] on **`panel_a`** to
///    recover the original action under the cursor. The mechanism is
///    "cursor-position memory": neither `tick_confirm` nor
///    `OpenEscMenuConfirmDialog` touches `panel_a.cursor_x/_y`, so it
///    still sits where the user clicked Force-SD / Draw / Quit in
///    state 1. `hit_test_cursor` re-runs on those preserved
///    coordinates and returns the originally-clicked item's kind.
///    `panel_a.slider_lock` is reliably 0 here — it's cleared on LMB
///    release in `tick_open`, so the only way to enter state 2 (a fresh
///    button click) guarantees the lock is already cleared.
///    The kind from that item drives the action:
///    * `0` (Force SD): `runtime._field_478 = 1`. Closes the menu.
///    * `1` (Draw): [`bridge_begin_round_end`]`(runtime, 1)`;
///      `runtime.game_end_phase = 2`. Closes the menu.
///    * `2` (Quit): `runtime.game_end_phase = 1`. If offline →
///      [`bridge_begin_round_end`]; if online →
///      [`bridge_begin_network_game_end`]. Closes the menu.
///    * Anything else / Miss → state stays 2 (waiting for another click).
pub unsafe fn tick_confirm(runtime: *mut GameRuntime) {
    unsafe {
        let world = (*runtime).world;

        // Step 1: input-focus gate (same as state 1).
        if !handle_at_input_focus(world, 0, (*runtime)._field_438) {
            return;
        }
        if !handle_at_input_focus(world, 2, (*runtime)._field_43c) {
            return;
        }

        // Step 2: Escape cancels — drop back to state 1.
        let keyboard = (*world).keyboard;
        if KeyboardAction::Escape.is_active2(keyboard) {
            (*runtime).esc_menu_state = 1;
            return;
        }

        // Step 3: poll mouse — consume + unconditional ack(3) + clear.
        let mouse_input = (*world).mouse_input;
        let mut dx: i32 = 0;
        let mut dy: i32 = 0;
        let mut buttons: u32 = 0;
        MouseInput::consume_delta_and_buttons_raw(mouse_input, &mut dx, &mut dy, &mut buttons);
        MouseInput::ack_button_mask_raw(mouse_input, 3);
        MouseInput::clear_deltas_raw(mouse_input);

        // Step 4: move panel-B's cursor.
        let panel_b = (*runtime).menu_panel_b;
        let new_x = (*panel_b).cursor_x + dx;
        let new_y = (*panel_b).cursor_y + dy;
        set_cursor_at(panel_b, new_x, new_y);

        // Step 5: bail unless LMB is pressed.
        if buttons & 1 == 0 {
            return;
        }

        // Step 6: activate on panel_b. Miss → miss sound + return.
        // Yes/No buttons return Button(kind); we map Slider(idx) to its
        // index so it threads through the same kind-value paths WA uses
        // (out_kind = idx for sliders), even though the confirm dialog
        // never builds a slider in practice.
        let kind_b = match activate_at_cursor(panel_b) {
            ActivateOutcome::Miss => {
                dispatch_global_sound(runtime, UI_MISS_SOUND, 8, Fixed::ONE, (*runtime).ui_volume);
                return;
            }
            ActivateOutcome::Button(k) => k,
            ActivateOutcome::Slider(idx) => idx,
        };
        dispatch_global_sound(runtime, UI_CLICK_SOUND, 8, Fixed::ONE, (*runtime).ui_volume);

        // No button (kind=1) → bounce back to the open menu.
        if kind_b == 1 {
            (*runtime).esc_menu_state = 1;
            return;
        }

        // Step 7: recover the original menu_panel_a action via slider_lock.
        let panel_a = (*runtime).menu_panel_a;
        let kind_a = match activate_at_cursor(panel_a) {
            ActivateOutcome::Miss => return,
            ActivateOutcome::Button(k) => k,
            ActivateOutcome::Slider(idx) => idx,
        };

        match kind_a {
            // Force Sudden Death confirmed.
            0 => {
                (*runtime)._field_478 = 1;
                (*runtime).esc_menu_state = 0;
            }
            // Draw This Round confirmed.
            1 => {
                bridge_begin_round_end(runtime, 1);
                (*runtime).game_end_phase = 2;
                (*runtime).esc_menu_state = 0;
            }
            // Quit The Game confirmed.
            2 => {
                (*runtime).game_end_phase = 1;
                if (*world).net_session.is_null() {
                    bridge_begin_round_end(runtime, 1);
                } else {
                    bridge_begin_network_game_end(runtime);
                }
                (*runtime).esc_menu_state = 0;
            }
            // Unknown kind / no original click recoverable: stay in state 2.
            _ => {}
        }
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
        WorldRootEntity::hud_data_query_raw(
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
        let panel_disp_a = (*panel).display_a;
        let pa_w = (*panel_disp_a).width as i32;
        let pa_h = (*panel_disp_a).height as i32;
        (*panel).cursor_active = 0;
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
        (*panel).slider_lock = 0;
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
        let volume_ptr = &raw mut (*runtime).sound_volume as *mut u8;
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
        DisplayBitGrid::put_pixel_clipped_raw(canvas, 0, 0, 0);
        DisplayBitGrid::put_pixel_clipped_raw(canvas, 0, final_y + 1, 0);
        DisplayBitGrid::put_pixel_clipped_raw(canvas, pw_minus_1, 0, 0);
        DisplayBitGrid::put_pixel_clipped_raw(canvas, pw_minus_1, final_y + 1, 0);

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

/// Rust port of `GameRuntime::OpenEscMenuConfirmDialog` (0x00535CF0).
///
/// Builds the Yes/No confirm overlay into `runtime.menu_panel_b` (with
/// `runtime.display_gfx_e` as its drawing canvas) and transitions
/// `esc_menu_state` to `2`. Called from [`tick_open`] when the user
/// clicks Force-SD / Draw / Quit.
///
/// Body shape (largely a smaller sibling of [`open_esc_menu`]):
/// 1. Background fill on the canvas with `gfx_color_table[7]`.
/// 2. Resolve the dialog title (token [`res::GAME_CONFIRM`]), measure,
///    draw centered at `((canvas_w - title_advance) / 2, 2)`.
/// 3. Two horizontal separators at `y = line_height + 3` and `+ 4`,
///    color `gfx_color_table[3]` (note: different from `OpenEscMenu`,
///    which uses index `[6]`).
/// 4. Reset `menu_panel_b` — clip rect to the canvas dims, cursor
///    centered to the canvas (different from `OpenEscMenu`'s reset
///    which clamps the existing cursor instead),
///    `cursor_active = 0`, `slider_lock = 0`, `item_count = 0`.
/// 5. Append the two buttons **side-by-side horizontally** at
///    `y = line_height + 7`:
///    - **Yes** (token [`res::GAME_YES`], kind=`0`) at `x = canvas_w/2 - 0x20`
///    - **No** (token [`res::GAME_NO`], kind=`1`) at `x = canvas_w/2 + 0x20`
/// 6. Park the cursor on the first kind=0 item (= Yes) via
///    [`center_cursor_on_first_kind_zero`] — Yes is the default selection.
/// 7. Border drawing — 4 hlines (thin top/bottom inset by 1px + wide
///    top/bottom) + 4 vlines + 4 corner pixels at color `0`. Same shape
///    as `OpenEscMenu`'s border. Final body height = `2*line_height + 12`.
/// 8. Final state writes:
///    - [`runtime.confirm_panel_width`](GameRuntime::confirm_panel_width)
///      = canvas_w
///    - [`runtime.confirm_panel_height`](GameRuntime::confirm_panel_height)
///      = `2*line_height + 12`
///    - Re-clamp the panel's clip rect + cursor to those dims.
///    - `esc_menu_state = 2`.
///
/// `__stdcall(this)`, RET 0x4 originally; the WA address has no
/// remaining xrefs once this port is wired in (the only caller was
/// `EscMenu_TickState1`, also Rust). Trapped in
/// `replacements/main_loop.rs` as a safety net.
pub unsafe fn open_confirm_dialog(runtime: *mut GameRuntime) {
    unsafe {
        let world = (*runtime).world;
        let display: *mut DisplayGfx = (*world).display;
        let canvas: *mut DisplayBitGrid = (*runtime).display_gfx_e;
        let panel: *mut MenuPanel = (*runtime).menu_panel_b;
        let template = (*world).localized_template;
        let bg_color = (*world).gfx_color_table[7] as u8;
        let border_color = (*world).gfx_color_table[3] as u8;
        let canvas_w = (*canvas).width as i32;
        let canvas_h = (*canvas).height as i32;

        // ─── Block A: background fill ───
        clipped_fill_rect(canvas, 0, 0, canvas_w, canvas_h, bg_color);

        // ─── Block B: title ───
        // WA resolves the GAME_CONFIRM token twice — once for measure,
        // once for draw. Resolve caches the result on the
        // `LocalizedTemplate`, so the second call is a hash lookup; we
        // collapse it to a single Rust call without changing semantics.
        let title_str = bridge_token_lookup(template, res::GAME_CONFIRM);
        let TextMeasurement {
            total_advance: title_advance,
            line_height,
        } = measure_text(display, 0xF, title_str).unwrap_or_default();
        let mut tmp_pen: i32 = 0;
        let mut tmp_w: i32 = 0;
        draw_text_on_bitmap(
            display,
            0xF,
            canvas,
            (canvas_w - title_advance) / 2,
            2,
            title_str,
            &mut tmp_pen,
            &mut tmp_w,
        );

        // ─── Block C: separators below title ───
        clipped_fill_hline(canvas, 0, canvas_w, line_height + 3, border_color);
        clipped_fill_hline(canvas, 0, canvas_w, line_height + 4, border_color);

        // ─── Block D: panel reset ───
        let panel_disp_a = (*panel).display_a;
        let pa_w = (*panel_disp_a).width as i32;
        let pa_h = (*panel_disp_a).height as i32;
        (*panel).cursor_active = 0;
        (*panel).clip_left = 0;
        (*panel).clip_top = 0;
        (*panel).clip_right = pa_w;
        (*panel).clip_bottom = pa_h;
        (*panel).cursor_x = pa_w / 2;
        (*panel).cursor_y = pa_h / 2;
        (*panel).slider_lock = 0;
        (*panel).item_count = 0;

        // ─── Block E: Yes / No buttons ───
        let button_y = line_height + 7;
        let center_x = canvas_w / 2;

        let label_yes = bridge_token_lookup(template, res::GAME_YES);
        append_item_impl(
            center_x - 0x20,
            panel,
            0, // kind = Yes
            label_yes,
            button_y,
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );

        let label_no = bridge_token_lookup(template, res::GAME_NO);
        append_item_impl(
            center_x + 0x20,
            panel,
            1, // kind = No
            label_no,
            button_y,
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );

        // ─── Block F: park cursor on Yes ───
        center_cursor_on_first_kind_zero(panel);

        // ─── Block G: border drawing ───
        let body_height = button_y + line_height + 3;
        let pw_minus_1 = canvas_w - 1;

        // 4 horizontal edges (thin outer, wide inner — same shape as
        // OpenEscMenu's border).
        clipped_fill_hline(canvas, 1, pw_minus_1, 0, border_color);
        clipped_fill_hline(canvas, 0, canvas_w, 1, border_color);
        clipped_fill_hline(canvas, 0, canvas_w, body_height, border_color);
        clipped_fill_hline(canvas, 1, pw_minus_1, body_height + 1, border_color);

        // 4 vertical edges.
        clipped_fill_vline(canvas, 0, 1, body_height, border_color);
        clipped_fill_vline(canvas, 1, 0, body_height + 1, border_color);
        clipped_fill_vline(canvas, pw_minus_1, 0, body_height + 1, border_color);
        clipped_fill_vline(canvas, canvas_w, 1, body_height, border_color);

        // 4 corner pixels @ color 0 to round off the border.
        DisplayBitGrid::put_pixel_clipped_raw(canvas, 0, 0, 0);
        DisplayBitGrid::put_pixel_clipped_raw(canvas, 0, body_height + 1, 0);
        DisplayBitGrid::put_pixel_clipped_raw(canvas, pw_minus_1, 0, 0);
        DisplayBitGrid::put_pixel_clipped_raw(canvas, pw_minus_1, body_height + 1, 0);

        // ─── Block H: final state writes ───
        (*runtime).confirm_panel_width = canvas_w;
        (*runtime).confirm_panel_height = body_height + 2;

        let cp_w = (*runtime).confirm_panel_width;
        let cp_h = (*runtime).confirm_panel_height;
        (*panel).clip_left = 0;
        (*panel).clip_top = 0;
        (*panel).clip_right = cp_w;
        (*panel).clip_bottom = cp_h;
        if (*panel).cursor_x < 0 {
            (*panel).cursor_x = 0;
        }
        if (*panel).cursor_y < 0 {
            (*panel).cursor_y = 0;
        }
        if cp_w < (*panel).cursor_x {
            (*panel).cursor_x = cp_w;
        }
        if cp_h < (*panel).cursor_y {
            (*panel).cursor_y = cp_h;
        }

        (*runtime).esc_menu_state = 2;
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

// ─── Per-frame render overlay ──────────────────────────────────────────────

/// Cursor-highlight sprite ID used by both ESC-menu render branches.
const CURSOR_HIGHLIGHT_SPRITE_ID: u16 = 0x20;
/// Palette value passed alongside the cursor sprite.
const CURSOR_HIGHLIGHT_PALETTE: u32 = 0xa000;
/// Flag bits passed to `draw_scaled_sprite` when blitting a panel canvas.
/// Bit 20 selects the Copy/opaque blend mode.
const PANEL_BLIT_FLAGS: u32 = 0x100000;
/// Base Y offset shared by both panels. Branch 2 blits the main menu
/// here directly; branch 1 builds the confirm dialog's Y by adding a
/// vertical-center delta + slide-in anim term to this base.
/// `-64.0` in Fixed16.16 — matches WA's `0xFFC00000` literal.
const PANEL_BASE_Y: Fixed = Fixed::from_int(-64);
/// Cursor offset *within* a panel before the panel position is added,
/// in pixels. WA uses `+10` on both axes in branch 1 (confirm) and on
/// the X axis of branch 2 (main menu).
const CURSOR_OFFSET_IN_PANEL: i32 = 10;

/// Rust port of `GameRuntime::RenderEscMenuOverlay` (0x00535000) — the
/// per-frame ESC-menu blit, called from `GameRender_Maybe` (0x533DC0) as
/// one of the tail render funcs.
///
/// Two independent gates animated separately:
///
/// 1. **Confirm dialog** — fires when [`GameRuntime::confirm_anim`] is
///    non-zero. Calls [`bridge_menu_panel_render`] on `menu_panel_b` to
///    redraw its canvas, then `display.draw_scaled_sprite` to blit the
///    canvas to screen at `(x_anchor, y_pos)`. Y position eases the dialog
///    down from above as `confirm_anim` slews `0 → 1.0`. When
///    `esc_menu_state == 2` the cursor-highlight sprite (id `0x20`,
///    palette `0xa000`) is drawn on top via `display.blit_sprite`.
/// 2. **Main menu** — fires when [`GameRuntime::esc_menu_anim`] is
///    non-zero. Same pattern with `menu_panel_a` at a fixed `y = -64.0`,
///    plus cursor highlight when `esc_menu_state == 1`. The cursor's Y
///    folds the panel's `-64` offset into a `-54` constant (`+10 - 64`).
///
/// `x_anchor` is shared between both branches: it slides the panel from
/// off-screen-left to centered (against `world.viewport_pixel_width`) as
/// `esc_menu_anim` slews. State 2 has `esc_menu_anim` at `1.0` too, so the
/// confirm dialog is anchored at the same X as the main menu underneath.
pub unsafe fn render_overlay(runtime: *mut GameRuntime) {
    unsafe {
        let world = (*runtime).world;
        let display = (*world).display;

        let viewport_w = (*world).viewport_pixel_width;
        let menu_panel_w = (*runtime).menu_panel_width;
        let esc_anim = (*runtime).esc_menu_anim;

        // X anchor (iVar5/EBX in the disasm) — Fixed. When
        // `esc_menu_anim == 1.0` (fully open) the panel is centered
        // horizontally with an 8px nudge. While slewing in, `slide_in`
        // shifts the panel off-screen to the left by `(menu_panel_w + 8)`
        // pixels at anim 0, fading to zero at anim 1.0. `Fixed * i32` is
        // `Fixed(raw * int)` — exactly WA's `(W+8) * (0x10000 - anim)`.
        let centered_x = Fixed::from_int(menu_panel_w / 2 - viewport_w / 2 + 8);
        let slide_in = (Fixed::ONE - esc_anim) * (menu_panel_w + 8);
        let x_anchor = centered_x - slide_in;

        // ─── Branch 1: confirm dialog ─────────────────────────────────
        let confirm_anim = (*runtime).confirm_anim;
        if confirm_anim != Fixed::ZERO {
            let confirm_w = (*runtime).confirm_panel_width;
            let confirm_h = (*runtime).confirm_panel_height;
            let menu_h = (*runtime).menu_panel_height;
            let panel_b = (*runtime).menu_panel_b;

            // Y position. WA: `((menu_h - confirm_h) << 16) / 2 +
            // (confirm_h + 8) * anim - 0x400000`. Re-expressed as Fixed:
            // vertical-center delta + anim slide-down + base panel Y.
            let center_delta = Fixed::from_int(menu_h - confirm_h) / 2;
            let slide_down = confirm_anim * (confirm_h + 8);
            let y_pos = center_delta + slide_down + PANEL_BASE_Y;

            let canvas = bridge_menu_panel_render(panel_b);

            DisplayGfx::draw_scaled_sprite_raw(
                display,
                x_anchor,
                y_pos,
                canvas,
                0,
                0,
                confirm_w,
                confirm_h,
                PANEL_BLIT_FLAGS,
            );

            if (*runtime).esc_menu_state == 2 {
                let cx = (*panel_b).cursor_x;
                let cy = (*panel_b).cursor_y;
                let cursor_x =
                    Fixed::from_int(cx - confirm_w / 2 + CURSOR_OFFSET_IN_PANEL) + x_anchor;
                let cursor_y = Fixed::from_int(cy - confirm_h / 2 + CURSOR_OFFSET_IN_PANEL) + y_pos;
                DisplayGfx::blit_sprite_raw(
                    display,
                    cursor_x,
                    cursor_y,
                    SpriteOp::from_index(CURSOR_HIGHLIGHT_SPRITE_ID),
                    CURSOR_HIGHLIGHT_PALETTE,
                );
            }
        }

        // ─── Branch 2: main ESC menu ──────────────────────────────────
        if esc_anim != Fixed::ZERO {
            let menu_h = (*runtime).menu_panel_height;
            let panel_a = (*runtime).menu_panel_a;
            let menu_w = (*runtime).menu_panel_width;

            let canvas = bridge_menu_panel_render(panel_a);

            DisplayGfx::draw_scaled_sprite_raw(
                display,
                x_anchor,
                PANEL_BASE_Y,
                canvas,
                0,
                0,
                menu_w,
                menu_h,
                PANEL_BLIT_FLAGS,
            );

            if (*runtime).esc_menu_state == 1 {
                let cx = (*panel_a).cursor_x;
                let cy = (*panel_a).cursor_y;
                let cursor_x = Fixed::from_int(cx - menu_w / 2 + CURSOR_OFFSET_IN_PANEL) + x_anchor;
                // The disasm shows `(cy - menu_h/2 - 54) << 16`: WA's
                // compiler folded `+10 - 64` into `-54` because the panel
                // Y is the constant `PANEL_BASE_Y`. Re-expanded here to
                // mirror branch 1's `cursor_in_panel + panel_y` shape.
                let cursor_y =
                    Fixed::from_int(cy - menu_h / 2 + CURSOR_OFFSET_IN_PANEL) + PANEL_BASE_Y;
                DisplayGfx::blit_sprite_raw(
                    display,
                    cursor_x,
                    cursor_y,
                    SpriteOp::from_index(CURSOR_HIGHLIGHT_SPRITE_ID),
                    CURSOR_HIGHLIGHT_PALETTE,
                );
            }
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
