//! Per-frame render entry point.
//!
//! Rust port of `GameRender_Maybe` (0x00533DC0) — the top-level render
//! coordinator called once per frame from `GameRuntime::render_frame`
//! (vtable slot 7, still bridged at 0x0056E040).
//!
//! Sequence:
//! 1. **Gate** on `world.render_skip_gate == 0` — early-exit if non-zero.
//! 2. **Stipple parity** toggle (alternates 0/1 each frame for dithered blends).
//! 3. **Display dimensions** — `display.get_dimensions()`.
//! 4. **Viewport pixel size** from level bounds, height rounded to even.
//! 5. **Letterbox bar height** — animated when `display_gfx_b` is non-null.
//! 6. **Save & clamp viewport dims** to display, save previous height.
//! 7. **Three [`clamp_camera_to_bounds`] calls** on `viewport_coords[0/1/3]`.
//! 8. **Reset RenderQueue** + `world.field_7ea0`.
//! 9. **Broadcast msg 3** to `world_root` — entities enqueue draw commands.
//! 10. **View-shake math** — sin/cos lerp scaled by `_field_7794`/`_field_7798`,
//!     added to `viewport_coords[3].center_x/_y` to build the stack
//!     ClipContext for queue dispatch.
//! 11. **Dispatch the queue** + post-RQ overlay sprites + letterbox frame
//!     + `RenderEscMenuOverlay` + the two network-only wait textboxes
//!     (`render_waiting_for_peers_textbox`,
//!     `render_network_end_wait_textbox`) + palette tail funcs.
//!
//! Headful-only paths (sections L+M, the viewport-frame letterbox block) are
//! gated on `g_GameSession.frame_state >= 0`.

use core::ffi::c_char;

use openwa_core::fixed::Fixed;
use openwa_core::trig;

use crate::address::va;
use crate::bitgrid::DisplayBitGrid;
use crate::engine::game_session::get_game_session;
use crate::engine::game_state;
use crate::engine::net_session::NetSession;
use crate::engine::runtime::GameRuntime;
use crate::engine::world::GameWorld;
use crate::entity::base::BaseEntity;
use crate::game::message::EntityMessage;
use crate::rebase::rb;
use crate::render::display::gfx::DisplayGfx;
use crate::render::palette::PaletteContext;
use crate::render::queue_dispatch::{ClipContext, clamp_camera_to_bounds, render_drawing_queue};
use crate::render::sprite::sprite_op::SpriteOp;
use crate::wa::localized_template::{LocalizedTemplate, resolve};
use crate::wa::sprintf_rotating::sprintf_3 as sprintf_rotating_3;
use crate::wa::string_resource::{StringRes, res};

// ─── Bridged WA helpers ────────────────────────────────────────────────────

static mut DRAW_AWAY_OVERLAY_ADDR: u32 = 0;
static mut SET_TEXTBOX_TEXT_ADDR: u32 = 0;
static mut PALETTE_ROTATE_HUES_ADDR: u32 = 0;
static mut PALETTE_BLEND_TOWARD_ADDR: u32 = 0;

/// Initialize the bridge addresses. Called from
/// `dispatch_frame::init_dispatch_addrs` at DLL load.
pub unsafe fn init_addrs() {
    unsafe {
        DRAW_AWAY_OVERLAY_ADDR = rb(va::GAME_RUNTIME_DRAW_AWAY_OVERLAY);
        SET_TEXTBOX_TEXT_ADDR = rb(va::SET_TEXTBOX_TEXT);
        PALETTE_ROTATE_HUES_ADDR = rb(va::PALETTE_CONTEXT_ROTATE_HUES);
        PALETTE_BLEND_TOWARD_ADDR = rb(va::PALETTE_CONTEXT_BLEND_TOWARD_COLOR);
    }
}

/// Bridge for `GameRuntime__DrawAwayOverlay_Maybe` (0x005336E0). Usercall:
/// `EDI = runtime`, `[stack] = top_y`, RET 0x4. Headful "GAME AWAY" /
/// "GAME OVER" overlay drawer; too many WA-side dependencies to port
/// incidentally.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_draw_away_overlay(_runtime: *mut GameRuntime, _top_y: i32) {
    core::arch::naked_asm!(
        "push edi",
        "mov edi, dword ptr [esp+8]",
        "push dword ptr [esp+16]",
        "call dword ptr [{addr}]",
        "pop edi",
        "ret 8",
        addr = sym DRAW_AWAY_OVERLAY_ADDR,
    );
}

/// Bridge for `SetTextboxText` (0x004FB070, stdcall RET 0x20). Renders
/// `text` into the textbox object's bitmap with two-tone shadow colors and
/// a per-call scale, returning the [`DisplayBitGrid`] canvas pointer plus
/// the pixel `(width, height)` consumed by the rendered text. Bridged
/// because the textbox-rendering code is large (~360 inst) and depends on
/// MFC font metrics; not worth porting incidentally.
unsafe fn set_textbox_text(
    textbox: *mut u8,
    text: *const c_char,
    color: u32,
    color_shadow_lo: u32,
    color_shadow_hi: u32,
    out_w: *mut i32,
    out_h: *mut i32,
    scale: Fixed,
) -> *mut DisplayBitGrid {
    unsafe {
        let func: unsafe extern "stdcall" fn(
            *mut u8,
            *const c_char,
            u32,
            u32,
            u32,
            *mut i32,
            *mut i32,
            Fixed,
        ) -> *mut DisplayBitGrid = core::mem::transmute(SET_TEXTBOX_TEXT_ADDR as usize);
        func(
            textbox,
            text,
            color,
            color_shadow_lo,
            color_shadow_hi,
            out_w,
            out_h,
            scale,
        )
    }
}

/// Bridge for `PaletteContext::RotateHues_Maybe` (0x005415A0, stdcall
/// RET 0x8). Walks `[dirty_range_min..=dirty_range_max]` of `ctx.rgb_table`,
/// converts each entry RGB→HLS, adds `frame_group` to the hue (mod 240),
/// converts back. Calls Win32 GDI's `ColorRGBToHLS`/`ColorHLSToRGB`.
unsafe fn palette_rotate_hues(ctx: *mut PaletteContext, frame_group: i32) {
    unsafe {
        let func: unsafe extern "stdcall" fn(*mut PaletteContext, i32) =
            core::mem::transmute(PALETTE_ROTATE_HUES_ADDR as usize);
        func(ctx, frame_group);
    }
}

/// Bridge for `PaletteContext::BlendTowardColor_Maybe` (0x005414F0).
/// Usercall(EAX = `alpha` clamped to `0..=0x10000`, [stack] = `ctx`,
/// `target_rgb`), RET 0x8. For each entry in
/// `[dirty_range_min..=dirty_range_max]`, lerps `ctx.rgb_table[i]` toward
/// `target_rgb` per channel by `alpha / 0x10000`.
#[unsafe(naked)]
unsafe extern "stdcall" fn palette_blend_toward(
    _ctx: *mut PaletteContext,
    _target_rgb: u32,
    _alpha: i32,
) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+0xC]",   // alpha
        "mov ecx, dword ptr [esp+0x8]",   // target_rgb
        "mov edx, dword ptr [esp+0x4]",   // ctx
        "push ecx",
        "push edx",
        "call dword ptr [{addr}]",         // RET 0x8 cleans the 2 pushes
        "ret 0xC",
        addr = sym PALETTE_BLEND_TOWARD_ADDR,
    );
}

// ─── Constants ─────────────────────────────────────────────────────────────

/// Sprite ID 0x253 (= 595) — central post-RQ overlay sprite.
const SPRITE_OVERLAY_CENTER: u16 = 0x253;
/// Sprite ID 0x46 (= 70) — turn / side indicator drawn when
/// `game_info._field_f37c == 0 && runtime._field_454 > 0`.
const SPRITE_TURN_INDICATOR: u16 = 0x46;

/// Stipple parity global at 0x007A087C — XOR'd with 1 each frame so
/// dithered-blend sprites alternate between two stipple patterns.
const G_STIPPLE_PARITY_VA: u32 = 0x007A087C;

// ─── Body ──────────────────────────────────────────────────────────────────

/// Rust port of `GameRender_Maybe` (0x00533DC0). See module docs.
pub unsafe fn game_render(runtime: *mut GameRuntime) {
    unsafe {
        let world = (*runtime).world;
        if (*world).render_skip_gate != 0 {
            return;
        }

        // ─── Section B: Stipple parity + display dims ─────────────────────
        let parity = rb(G_STIPPLE_PARITY_VA) as *mut u32;
        *parity ^= 1;

        let display = (*world).display;
        let mut display_w: u32 = 0;
        let mut display_h: u32 = 0;
        DisplayGfx::get_dimensions_raw(display, &mut display_w, &mut display_h);
        let display_w = display_w as i32;
        let display_h = display_h as i32;

        // ─── Section C: viewport pixel dims from level bounds ─────────────
        let level_min_x = (*world).level_bound_min_x;
        let level_max_x = (*world).level_bound_max_x;
        let level_min_y = (*world).level_bound_min_y;
        let level_max_y = (*world).level_bound_max_y;
        let mut viewport_w_px = (level_max_x - level_min_x).to_int();
        // Height rounds down to an even pixel count to keep odd/even rows
        // consistent with WA's two-pass dithered renderer.
        let mut viewport_h_px = (level_max_y - level_min_y).to_int() & !1;

        // ─── Section D: letterbox bar height ──────────────────────────────
        let bar_height = compute_bar_height(runtime);

        // ─── Section E: save & clamp viewport dims ────────────────────────
        // Save current `viewport_pixel_height` to `..._prev` BEFORE
        // overwriting; consumers compare to detect height changes between
        // frames.
        (*world).viewport_pixel_height_prev = (*world).viewport_pixel_height;
        if viewport_w_px > display_w {
            viewport_w_px = display_w;
        }
        (*world).viewport_pixel_width = viewport_w_px;
        let viewport_h_max = display_h - bar_height;
        if viewport_h_px > viewport_h_max {
            viewport_h_px = viewport_h_max;
        }
        (*world).viewport_pixel_height = viewport_h_px;
        // First frame: prev was 0 → treat current as the previous so
        // downstream code doesn't see a spurious "height changed" event.
        if (*world).viewport_pixel_height_prev == 0 {
            (*world).viewport_pixel_height_prev = (*world).viewport_pixel_height;
        }

        // ─── Section F: clamp camera anchors on three viewport coords ─────
        // Skips index 2 — it's owned by a different subsystem. Indices 0
        // and 1 are HUD-related; 3 is the main game-camera anchor whose
        // post-clamp value drives the queue dispatch below.
        for i in [0usize, 1, 3] {
            clamp_camera_to_bounds(
                &mut (*world).viewport_coords[i],
                viewport_w_px,
                viewport_h_px,
                level_min_x,
                level_min_y,
                level_max_x,
                level_max_y,
            );
        }

        // ─── Section G: reset RenderQueue ─────────────────────────────────
        let rq = (*world).render_queue;
        (*rq).entry_count = 0;
        (*rq).buffer_offset = 0x10000;

        // ─── Section H: reset per-frame counter ───────────────────────────
        // `field_7ea0` is incremented by some msg-3 handler; read after
        // the broadcast to set `runtime._field_458`'s sign.
        (*world).field_7ea0 = 0;

        // ─── Section I: dispatch "render this frame" message to world_root ─
        // Calls world_root's vtable[2] (HandleMessage), which propagates
        // msg 3 down the entity tree; each entity responds by enqueueing
        // draw commands into the RQ.
        let world_root = (*runtime).world_root as *mut BaseEntity;
        BaseEntity::handle_message_raw(
            world_root,
            core::ptr::null_mut(),
            EntityMessage::RenderScene,
            0,
            core::ptr::null(),
        );

        // ─── Section J/K: build stack ClipContext with view shake ─────────
        let mut clip = build_view_shake_clip(world);

        // ─── Headful gate: bail out before queue dispatch on headless ────
        let session = get_game_session();
        if (*session).frame_state < 0 {
            return;
        }

        // ─── Section L: dispatch the render queue ─────────────────────────
        render_drawing_queue(rq, display, &mut clip);

        // ─── Section L1: post-RQ central overlay sprite ───────────────────
        draw_post_rq_overlay(runtime, world);

        // ─── Section L2: turn-indicator sign update + conditional sprite ──
        update_turn_indicator(runtime, world);

        // ─── Section M: letterbox / viewport-frame draws ──────────────────
        draw_viewport_frame(runtime, world, display_w, display_h, bar_height);

        // ─── Section N: tail render funcs ─────────────────────────────────
        crate::engine::main_loop::esc_menu::render_overlay(runtime);
        render_waiting_for_peers_textbox(runtime);
        render_network_end_wait_textbox(runtime);
        palette_manage(runtime);
        palette_animate(runtime);
    }
}

/// Section D — letterbox bar height. Animates between two configurations
/// using `runtime._field_3fc` (turn-timer slew Fixed) when `display_gfx_b`
/// is non-null. Returns 0 when `display_gfx_b` is null.
unsafe fn compute_bar_height(runtime: *mut GameRuntime) -> i32 {
    unsafe {
        if (*runtime).display_gfx_b.is_null() {
            return 0;
        }
        let max_idx = (*runtime).max_team_render_index;
        let initial = if (*runtime).hud_team_bar_extended == 0 {
            0
        } else {
            ((*runtime).worm_select_count_alt + 1) * max_idx + 6
        };
        let target = ((*runtime).worm_select_count + 1) * max_idx;
        // (target - initial + 6) * slew >> 16, then + initial, rounded to even.
        // Faithful to WA's `(((field_2a0+1)*field_2b0 - bar_initial) + 6) * field_3fc >> 16`.
        let delta = (target - initial + 6).wrapping_mul((*runtime).chat_box_anim.to_raw()) >> 16;
        (delta + initial) & !1
    }
}

/// Sections J/K — copy `viewport_coords[3]` into a stack ClipContext, then
/// add a per-frame view-shake offset to its `cam_x`/`cam_y`:
///
/// ```text
/// t      = ((world.frame_counter << 16) + world.render_interp_b) * 7 / 50
/// shake  = cos_lerp(t & 0xFFFF) * world._field_7794   // for X
/// shake' = sin_lerp(t & 0xFFFF) * world._field_7798   // for Y
/// ```
///
/// `_field_7794` / `_field_7798` are amplitude scalers (zeroed in normal
/// gameplay, non-zero during explosion / damage / earthquake events). The
/// pivot fields are copied verbatim from `viewport_coords[3]`.
unsafe fn build_view_shake_clip(world: *mut GameWorld) -> ClipContext {
    unsafe {
        let coords3 = &(*world).viewport_coords[3];
        let mut clip = ClipContext {
            cam_x: coords3.center_x,
            cam_y: coords3.center_y,
            pivot_x: coords3.center_x_target,
            pivot_y: coords3.center_y_target,
        };

        // 7/50 factor matches WA's `LEA EDX,[EAX*8 + 0]; SUB EDX,EAX;
        // IMUL ...0x51eb851f...` (i32 mul-by-7 + reciprocal divide-by-50).
        let frame_t = ((*world).frame as i32)
            .wrapping_shl(16)
            .wrapping_add((*world).render_interp_b.to_raw());
        let t = frame_t.wrapping_mul(7) / 50;

        // The trig helpers use bits 0..16 of the angle (10 index + 6 frac).
        let angle = t as u32;
        let sin_lerp = trig::sin(angle);
        let cos_lerp = trig::cos(angle);

        let amp_x = (*world).shake_intensity_x;
        let amp_y = (*world).shake_intensity_y;

        clip.cam_x += cos_lerp.mul_raw(amp_x);
        clip.cam_y += sin_lerp.mul_raw(amp_y);

        clip
    }
}

/// Section L1 — central post-RQ overlay sprite. Drawn unconditionally on
/// the headful path with sprite ID 0x253 at:
///
/// ```text
/// x = (((viewport_w/2 + 0x20) << 10) - runtime._field_27c) << 6
/// y = (0x10 - viewport_h/2) << 16
/// palette = world.scaled_frame_accum / 50  (low 16 bits)
/// ```
unsafe fn draw_post_rq_overlay(runtime: *mut GameRuntime, world: *mut GameWorld) {
    unsafe {
        let display = (*world).display;
        let viewport_w = (*world).viewport_pixel_width;
        let viewport_h = (*world).viewport_pixel_height;
        let palette = ((*world).scaled_frame_accum.to_raw() / 50) as u32 & 0xFFFF;

        let y = Fixed::from_int(0x10 - viewport_h / 2);
        // The X expression is unusual (mixes `<< 10` and `<< 6`); kept
        // verbatim from WA. Yields a Fixed16.16 X position in the camera
        // frame after `_field_27c`'s sub-pixel adjustment.
        let x_raw =
            ((viewport_w / 2 + 0x20) << 10).wrapping_sub((*runtime).connection_issue_anim.to_raw());
        let x = Fixed::from_raw(x_raw << 6);

        DisplayGfx::blit_sprite_raw(
            display,
            x,
            y,
            SpriteOp::from_index(SPRITE_OVERLAY_CENTER),
            palette,
        );
    }
}

/// Section L2 — turn / side indicator. When `runtime._field_454 == 0`,
/// recomputes `runtime._field_458` as the sign of `world.field_7ea0`
/// (`+1` if any entity bumped the counter during msg-3 broadcast, `-1`
/// otherwise). When `_field_454 > 0` AND `world.game_info._field_f37c == 0`,
/// blits sprite 0x46 at a sign-flipped X (using `_field_458`) and a
/// vertically-easing Y based on `_field_454`.
unsafe fn update_turn_indicator(runtime: *mut GameRuntime, world: *mut GameWorld) {
    unsafe {
        let anim = (*runtime).message_indicator_anim;

        if anim == Fixed::ZERO {
            (*runtime)._field_458 = if (*world).field_7ea0 > 0 { 1 } else { -1 } as u32;
        }

        let game_info = (*world).game_info;
        // `game_info._field_f37c` lives in the unmapped middle of GameInfo;
        // accessed here via a raw byte offset until the field is broken out.
        let field_f37c = *((game_info as *const u8).add(0xF37C) as *const i32);
        if field_f37c != 0 || anim <= Fixed::ZERO {
            return;
        }

        let display = (*world).display;
        let viewport_w = (*world).viewport_pixel_width;
        let viewport_h = (*world).viewport_pixel_height;
        let palette = ((*world).scaled_frame_accum.to_raw() / 50) as u32;

        // y = field_454 * 0x46 - ((vh/2 + 0x23) << 16)
        let y_raw = anim
            .to_raw()
            .wrapping_mul(0x46)
            .wrapping_sub((viewport_h / 2 + 0x23) << 16);
        // x = (vw/2 - 0x23) * sign(_field_458) << 16
        let sign = (*runtime)._field_458 as i32;
        let x_raw = ((viewport_w / 2 - 0x23).wrapping_mul(sign)) << 16;

        DisplayGfx::blit_sprite_raw(
            display,
            Fixed::from_raw(x_raw),
            Fixed::from_raw(y_raw),
            SpriteOp::from_index(SPRITE_TURN_INDICATOR),
            palette,
        );
    }
}

/// Section M — viewport-frame letterbox draws + final camera/clip setup.
///
/// Splits on `runtime.display_gfx_b == 0`:
/// - **Without HUD** (the simpler branch): draws horizontal bars on the
///   left/right when there's a horizontal margin, then a top/bottom pair
///   when there's a vertical margin, then sets the camera to the screen
///   center.
/// - **With HUD**: clears the whole display via `set_camera_offset(0,0)` +
///   `set_clip_rect(0, 0, display_w, display_h)`, then a clamped top
///   `fill_rect`, a bottom `fill_rect`, optional left/right bars,
///   followed by a HUD-centered camera/clip setup, the
///   `DrawAwayOverlay_Maybe` bridge call, and a re-center on the playfield.
///
/// Both paths end with a final `set_clip_rect(margin, margin,
/// margin + viewport_w, margin + viewport_h)` so the playfield is clipped
/// to its exact rectangle for the tail render funcs.
unsafe fn draw_viewport_frame(
    runtime: *mut GameRuntime,
    world: *mut GameWorld,
    display_w: i32,
    display_h: i32,
    bar_height: i32,
) {
    unsafe {
        let display = (*world).display;
        let viewport_w = (*world).viewport_pixel_width;
        let viewport_h = (*world).viewport_pixel_height;

        // Horizontal margin (split equally on both sides) and vertical
        // margin (above/below the playfield, accounting for the HUD bar).
        let h_margin = (display_w - viewport_w) / 2;
        let mut v_margin = (display_h - viewport_h) / 2;

        if !(*runtime).display_gfx_b.is_null() {
            // ── With-HUD path ──
            DisplayGfx::set_camera_offset_raw(display, Fixed::ZERO, Fixed::ZERO);
            DisplayGfx::set_clip_rect_raw(
                display,
                Fixed::ZERO,
                Fixed::ZERO,
                Fixed::from_int(display_w),
                Fixed::from_int(display_h),
            );

            // Top fill_rect runs only when the vertical margin exceeds the
            // bar height (otherwise the bar already covers the same area).
            if v_margin >= bar_height {
                DisplayGfx::fill_rect_raw(display, 0, bar_height, display_w, v_margin, 0);
            } else {
                v_margin = bar_height;
            }
            // Bottom fill_rect (always runs).
            DisplayGfx::fill_rect_raw(display, 0, viewport_h + v_margin, display_w, display_h, 0);
            // Left/right bars (only when there's a horizontal margin).
            if h_margin != 0 {
                DisplayGfx::fill_rect_raw(display, 0, v_margin, h_margin, viewport_h, 0);
                DisplayGfx::fill_rect_raw(
                    display,
                    h_margin + viewport_w,
                    v_margin,
                    display_w,
                    viewport_h,
                    0,
                );
            }

            // HUD area camera/clip setup. `display_gfx_b._field_18` is the
            // HUD bar's pixel height (read raw — DisplayGfxLayer's typed
            // wrapper isn't surfaced yet).
            let hud_bar_h_ptr = (*runtime).display_gfx_b.add(0x18) as *const i32;
            let hud_bar_h = *hud_bar_h_ptr;
            DisplayGfx::set_camera_offset_raw(
                display,
                Fixed::from_int(display_w / 2),
                Fixed::from_int(bar_height - hud_bar_h / 2),
            );
            DisplayGfx::set_clip_rect_raw(
                display,
                Fixed::ZERO,
                Fixed::ZERO,
                Fixed::from_int(display_w),
                Fixed::from_int(bar_height),
            );

            bridge_draw_away_overlay(runtime, bar_height);

            // Re-center camera onto the playfield (post-HUD).
            DisplayGfx::set_camera_offset_raw(
                display,
                Fixed::from_int(display_w / 2),
                Fixed::from_int(viewport_h / 2 + v_margin),
            );
        } else {
            // ── Without-HUD path ──
            // Horizontal bars first (only when there's a horizontal margin).
            if h_margin != 0 {
                DisplayGfx::set_camera_offset_raw(display, Fixed::ZERO, Fixed::ZERO);
                DisplayGfx::set_clip_rect_raw(
                    display,
                    Fixed::ZERO,
                    Fixed::ZERO,
                    Fixed::from_int(display_w),
                    Fixed::from_int(display_h),
                );
                DisplayGfx::fill_rect_raw(display, 0, v_margin, h_margin, display_h - v_margin, 0);
                DisplayGfx::fill_rect_raw(
                    display,
                    display_w - h_margin,
                    v_margin,
                    display_w,
                    display_h - v_margin,
                    0,
                );
            }
            // Vertical bars (top/bottom).
            if v_margin != 0 {
                DisplayGfx::set_camera_offset_raw(display, Fixed::ZERO, Fixed::ZERO);
                DisplayGfx::set_clip_rect_raw(
                    display,
                    Fixed::ZERO,
                    Fixed::ZERO,
                    Fixed::from_int(display_w),
                    Fixed::from_int(display_h),
                );
                DisplayGfx::fill_rect_raw(display, 0, 0, display_w, v_margin, 0);
                DisplayGfx::fill_rect_raw(
                    display,
                    0,
                    display_h - v_margin,
                    display_w,
                    display_h,
                    0,
                );
            }
            DisplayGfx::set_camera_offset_raw(
                display,
                Fixed::from_int(display_w / 2),
                Fixed::from_int(display_h / 2),
            );
        }

        // Final clip rect — clamps to the exact playfield rectangle for
        // the tail render funcs.
        DisplayGfx::set_clip_rect_raw(
            display,
            Fixed::from_int(h_margin),
            Fixed::from_int(v_margin),
            Fixed::from_int(viewport_w + h_margin),
            Fixed::from_int(viewport_h + v_margin),
        );
    }
}

// ─── Section N tail funcs (ported) ─────────────────────────────────────────

/// Two-tone "blink" color for the two network wait textboxes
/// ([`render_waiting_for_peers_textbox`] /
/// [`render_network_end_wait_textbox`]). WA's
/// shape is `(-(uint)((tick / 25 & 1) != 0) & 0xFFFFFFFA) + 6`, which
/// simplifies to: `0` on every other 25-frame group, `6` otherwise.
fn blink_color(tick: i32) -> u32 {
    if (tick / 25) & 1 != 0 { 0 } else { 6 }
}

/// Rust port of `GameRuntime__RenderWaitingForPeersTextbox` (0x00534F20).
/// Usercall(EAX = runtime), plain RET.
///
/// Despite the original `RenderHUD_Maybe` name (since corrected in Ghidra),
/// this function does NOT render the in-game HUD — it draws exactly one
/// localized textbox (`GAME_PLEASE_WAIT`), centered horizontally at
/// `y = 0x40 - viewport_h/2` (Fixed pixels), and only during the very
/// specific pre-round window when:
/// - we're in network play (`world.net_session != null`),
/// - the round is in `game_state == 1` (running), AND
/// - [`super::dispatch_frame::all_peer_teams_have_joined`] still returns
///   `false` — i.e. at least one peer's team hasn't had its starting
///   marker set yet.
///
/// Once all peers have joined, this function silently returns and the
/// textbox disappears.
unsafe fn render_waiting_for_peers_textbox(runtime: *mut GameRuntime) {
    unsafe {
        if (*runtime).game_state != 1 {
            return;
        }
        let world = (*runtime).world;
        if (*world).net_session.is_null() {
            return;
        }
        if super::dispatch_frame::all_peer_teams_have_joined(runtime) {
            return;
        }

        draw_textbox_overlay(runtime, world, res::GAME_PLEASE_WAIT, None);
    }
}

/// Rust port of `GameRuntime__RenderNetworkEndWaitTextbox` (0x00534E00).
/// Usercall(EAX = runtime), plain RET.
///
/// Despite the original `RenderTurnStatus_Maybe` name (since corrected in
/// Ghidra), this function does NOT render any kind of turn status — it
/// draws exactly one localized textbox (`GAME_PLAYER_WAIT`,
/// `"PLEASE WAIT %d SEC"`) during the network end-of-round handshake.
///
/// Fires when:
/// - `game_state` is [`game_state::NETWORK_END_STARTED`] (3) or
///   [`game_state::NETWORK_END_AWAITING_PEERS`] (2),
/// - `world.net_session != null`, AND
/// - either `runtime.net_end_countdown != 0` (still in the timeout
///   window), OR the countdown has reached zero AND
///   [`NetSession::end_handshake_busy_maybe`] reports the handshake is no
///   longer busy (the textbox stays as "PLEASE WAIT 0 SEC" while we
///   linger waiting for state transition; if the predicate signals
///   actual work in flight after the countdown expires we suppress the
///   textbox).
///
/// The countdown shown is `net_end_countdown / 50` (frames → seconds),
/// formatted into the template via the rotating sprintf scratch buffer.
///
/// Companion of [`render_waiting_for_peers_textbox`] (the *pre*-round
/// wait); this one handles the *post*-round wait.
unsafe fn render_network_end_wait_textbox(runtime: *mut GameRuntime) {
    unsafe {
        let state = (*runtime).game_state;
        if state != game_state::NETWORK_END_AWAITING_PEERS
            && state != game_state::NETWORK_END_STARTED
        {
            return;
        }
        let world = (*runtime).world;
        let net_session = (*world).net_session;
        if net_session.is_null() {
            return;
        }
        let countdown = (*runtime).net_end_countdown;
        if countdown == 0 {
            // Countdown expired: poll the predicate and bail out if busy.
            let busy = NetSession::end_handshake_busy_maybe_raw(net_session);
            if busy != 0 {
                return;
            }
        }

        // `net_end_countdown / 50` — seconds remaining, formatted into the
        // template via the rotating sprintf scratch buffer.
        let seconds = countdown / 50;
        draw_textbox_overlay(runtime, world, res::GAME_PLAYER_WAIT, Some(seconds));
    }
}

/// Shared body of [`render_waiting_for_peers_textbox`] /
/// [`render_network_end_wait_textbox`]: resolves the
/// localized template, optionally formats it with `format_arg`, lays it
/// out via [`set_textbox_text`], and blits the resulting bitmap centered
/// horizontally on the playfield.
unsafe fn draw_textbox_overlay(
    _runtime: *mut GameRuntime,
    world: *mut GameWorld,
    token_id: StringRes,
    format_arg: Option<i32>,
) {
    unsafe {
        let template: *mut LocalizedTemplate = (*world).localized_template;
        let mut text = resolve(template, token_id);
        if let Some(arg) = format_arg {
            // WA pushes 3 varargs even when the format string consumes one.
            text = sprintf_rotating_3(text, 1, 0, arg as u32);
        }

        let tick = (*world).frame as i32;
        let color = blink_color(tick);
        let textbox = (*world).textbox;
        let shadow_lo = (*world).gfx_color_table[7];
        let shadow_hi = (*world).gfx_color_table[6];
        let mut text_w: i32 = 0;
        let mut text_h: i32 = 0;
        let sprite = set_textbox_text(
            textbox,
            text,
            color,
            shadow_lo,
            shadow_hi,
            &mut text_w,
            &mut text_h,
            Fixed::ONE,
        );

        let display = (*world).display;
        let y = Fixed::from_int(0x40 - (*world).viewport_pixel_height / 2);
        DisplayGfx::draw_scaled_sprite_raw(
            display,
            Fixed::ZERO,
            y,
            sprite,
            0,
            0,
            text_w,
            text_h,
            0x100000,
        );
    }
}

/// Global byte at `0x007A085E` — master enable for [`palette_manage`].
/// Set by gameplay code (e.g. on round start); cleared on hardware
/// palette failure. WA's original [`palette_manage`] body was wrapped in
/// MSVC SEH that cleared this byte on `0xC06D007E/F` C++ exceptions
/// thrown by the WA implementations of `set_active_layer` /
/// `update_palette` — both ported to safe Rust now, so the SEH guard is
/// dropped from this port.
const G_PALETTE_ANIM_ENABLED_VA: u32 = 0x007A085E;
/// Global counter at `0x0077499C` — incremented every frame inside
/// [`palette_manage`]; the body fires once every 50 increments.
const G_PALETTE_ANIM_TICK_VA: u32 = 0x0077499C;

/// Rust port of `PaletteManage_Maybe` (0x00533C80). Stdcall(runtime),
/// RET 0x4. Once the global tick counter at [`G_PALETTE_ANIM_TICK_VA`]
/// crosses a 50-frame boundary, copies layer-2's palette state into
/// `runtime.palette_ctx_b`, applies a frame-group hue rotation via
/// [`palette_rotate_hues`], then commits the result through the display
/// vtable's `update_palette`.
///
/// The original SEH guard around the rotate+commit is omitted — see
/// [`G_PALETTE_ANIM_ENABLED_VA`] for the rationale.
unsafe fn palette_manage(runtime: *mut GameRuntime) {
    unsafe {
        let enabled_ptr = rb(G_PALETTE_ANIM_ENABLED_VA) as *mut u8;
        if *enabled_ptr == 0 {
            return;
        }
        let tick_ptr = rb(G_PALETTE_ANIM_TICK_VA) as *mut u32;
        let tick = (*tick_ptr).wrapping_add(1);
        *tick_ptr = tick;
        // Original body: `if (tick == (tick/50)*50)`. With wrapping
        // semantics that's exactly `tick % 50 == 0`.
        if !tick.is_multiple_of(50) {
            return;
        }

        let world = (*runtime).world;
        let display = (*world).display;
        let layer_palette = DisplayGfx::set_active_layer_raw(display, 2);
        let dst = (*runtime).palette_ctx_b;
        // 0x1C3 dwords = 0x70C bytes = `size_of::<PaletteContext>()`.
        core::ptr::copy_nonoverlapping(layer_palette as *const u32, dst as *mut u32, 0x1C3);

        let frame_group = (tick / 50) as i32;
        palette_rotate_hues(dst, frame_group);
        DisplayGfx::update_palette_raw(display, dst, 1);
    }
}

/// Rust port of `PaletteAnimate_Maybe` (0x00533A80). Stdcall(runtime),
/// RET 0x4. Recomputes the three layer palettes (a/b/c) when any of the
/// three cached fade inputs has changed since the last frame, then commits
/// them. Drives the screen fade-to-black at game-end and during pause.
///
/// Cached state on `GameRuntime`:
/// - `_field_468` ← `world._field_7390` (per-frame UI fade-in alpha?)
/// - `_field_46c` ← `max(world.render_scale, runtime.game_end_speed)`
///   (the larger of the live render-scale and the game-end fade)
/// - `_field_470` ← `world._field_7398` (per-frame world fade alpha?)
///
/// On a cache miss, for each layer (1 → ctx_a, 2 → ctx_b, 3 → ctx_c):
/// 1. Copy the layer's palette state from `display.set_active_layer(N)`
///    into the runtime-owned [`PaletteContext`].
/// 2. Apply 2-3 [`palette_blend_toward`] calls toward the target color
///    `0` (black) with per-layer alphas.
/// 3. Commit via `update_palette`. Layer 3's commit pushes to the DDraw
///    surface (`commit = 1`); layers 1-2 only update internal state.
unsafe fn palette_animate(runtime: *mut GameRuntime) {
    unsafe {
        let world = (*runtime).world;
        let render_scale = (*world).render_scale.to_raw();
        let game_end_speed = (*runtime).game_end_speed.0;
        let clamp = render_scale.max(game_end_speed);

        let cur_a = (*world)._field_7390 as i32;
        let cur_c = (*world)._field_7398 as i32;
        if (*runtime)._field_468 == cur_a
            && (*runtime)._field_46c == clamp
            && (*runtime)._field_470 == cur_c
        {
            return;
        }

        let display = (*world).display;
        const PALETTE_CTX_DWORDS: usize = 0x1C3;
        const TARGET_BLACK: u32 = 0;

        // ── Layer 1 → palette_ctx_a ─────────────────────────────────────
        let ctx_a = (*runtime).palette_ctx_a;
        let src1 = DisplayGfx::set_active_layer_raw(display, 1);
        core::ptr::copy_nonoverlapping(src1 as *const u32, ctx_a as *mut u32, PALETTE_CTX_DWORDS);
        palette_blend_toward(ctx_a, TARGET_BLACK, cur_c);
        palette_blend_toward(ctx_a, TARGET_BLACK, clamp);

        // ── Layer 2 → palette_ctx_b ─────────────────────────────────────
        let ctx_b = (*runtime).palette_ctx_b;
        let src2 = DisplayGfx::set_active_layer_raw(display, 2);
        core::ptr::copy_nonoverlapping(src2 as *const u32, ctx_b as *mut u32, PALETTE_CTX_DWORDS);
        palette_blend_toward(ctx_b, TARGET_BLACK, cur_a);
        palette_blend_toward(ctx_b, TARGET_BLACK, cur_c);
        palette_blend_toward(ctx_b, TARGET_BLACK, clamp);

        // ── Layer 3 → palette_ctx_c ─────────────────────────────────────
        let ctx_c = (*runtime).palette_ctx_c;
        let src3 = DisplayGfx::set_active_layer_raw(display, 3);
        core::ptr::copy_nonoverlapping(src3 as *const u32, ctx_c as *mut u32, PALETTE_CTX_DWORDS);
        palette_blend_toward(ctx_c, TARGET_BLACK, cur_c);
        palette_blend_toward(ctx_c, TARGET_BLACK, clamp);

        // ── Commit (only the last call pushes to DDraw) ────────────────
        DisplayGfx::update_palette_raw(display, ctx_a, 0);
        DisplayGfx::update_palette_raw(display, ctx_b, 0);
        DisplayGfx::update_palette_raw(display, ctx_c, 1);

        // ── Update cache ────────────────────────────────────────────────
        (*runtime)._field_468 = cur_a;
        (*runtime)._field_46c = clamp;
        (*runtime)._field_470 = cur_c;
    }
}
