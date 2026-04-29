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
//!     + `RenderEscMenuOverlay` + `RenderHUD` + `RenderTurnStatus` +
//!     palette tail funcs (last 5 still bridged).
//!
//! Headful-only paths (sections L+M, the viewport-frame letterbox block) are
//! gated on `g_GameSession.frame_state >= 0`.

use openwa_core::fixed::Fixed;
use openwa_core::trig;

use crate::address::va;
use crate::engine::game_session::get_game_session;
use crate::engine::runtime::GameRuntime;
use crate::engine::world::GameWorld;
use crate::game::message::TaskMessage;
use crate::rebase::rb;
use crate::render::display::gfx::DisplayGfx;
use crate::render::queue_dispatch::{ClipContext, clamp_camera_to_bounds, render_drawing_queue};
use crate::render::sprite::sprite_op::SpriteOp;
use crate::task::base::BaseEntity;

// ─── Bridged WA helpers ────────────────────────────────────────────────────

static mut DRAW_AWAY_OVERLAY_ADDR: u32 = 0;
static mut RENDER_HUD_ADDR: u32 = 0;
static mut RENDER_TURN_STATUS_ADDR: u32 = 0;
static mut PALETTE_MANAGE_ADDR: u32 = 0;
static mut PALETTE_ANIMATE_ADDR: u32 = 0;

/// Initialize the bridge addresses. Called from
/// `dispatch_frame::init_dispatch_addrs` at DLL load.
pub unsafe fn init_addrs() {
    unsafe {
        DRAW_AWAY_OVERLAY_ADDR = rb(va::GAME_RUNTIME_DRAW_AWAY_OVERLAY);
        RENDER_HUD_ADDR = rb(va::RENDER_HUD_MAYBE);
        RENDER_TURN_STATUS_ADDR = rb(va::RENDER_TURN_STATUS_MAYBE);
        PALETTE_MANAGE_ADDR = rb(va::PALETTE_MANAGE_MAYBE);
        PALETTE_ANIMATE_ADDR = rb(va::PALETTE_ANIMATE_MAYBE);
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

/// Bridge for `RenderHUD_Maybe` (0x00534F20). Usercall: `EAX = runtime`,
/// no stack args, plain RET. Draws the "GAME OVER" textbox when
/// `runtime.game_state == 1`.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_render_hud(_runtime: *mut GameRuntime) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",
        "call dword ptr [{addr}]",
        "ret 4",
        addr = sym RENDER_HUD_ADDR,
    );
}

/// Bridge for `RenderTurnStatus_Maybe` (0x00534E00). Usercall:
/// `EAX = runtime`, no stack args, plain RET. Draws the turn-status text
/// when `runtime.game_state` is 2 or 3.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_render_turn_status(_runtime: *mut GameRuntime) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",
        "call dword ptr [{addr}]",
        "ret 4",
        addr = sym RENDER_TURN_STATUS_ADDR,
    );
}

/// Bridge for `PaletteManage_Maybe` (0x00533C80). Stdcall RET 0x4 (one
/// stack arg = runtime).
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_palette_manage(_runtime: *mut GameRuntime) {
    core::arch::naked_asm!(
        "jmp dword ptr [{addr}]",
        addr = sym PALETTE_MANAGE_ADDR,
    );
}

/// Bridge for `PaletteAnimate_Maybe` (0x00533A80). Stdcall RET 0x4.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_palette_animate(_runtime: *mut GameRuntime) {
    core::arch::naked_asm!(
        "jmp dword ptr [{addr}]",
        addr = sym PALETTE_ANIMATE_ADDR,
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
            TaskMessage::RenderScene,
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
        bridge_render_hud(runtime);
        bridge_render_turn_status(runtime);
        bridge_palette_manage(runtime);
        bridge_palette_animate(runtime);
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
        let initial = if (*runtime)._field_414 == 0 {
            0
        } else {
            ((*runtime).worm_select_count_alt + 1) * max_idx + 6
        };
        let target = ((*runtime).worm_select_count + 1) * max_idx;
        // (target - initial + 6) * slew >> 16, then + initial, rounded to even.
        // Faithful to WA's `(((field_2a0+1)*field_2b0 - bar_initial) + 6) * field_3fc >> 16`.
        let delta = (target - initial + 6).wrapping_mul((*runtime)._field_3fc.to_raw()) >> 16;
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
        let frame_t = ((*world)._field_77d4 as i32)
            .wrapping_shl(16)
            .wrapping_add((*world).render_interp_b.to_raw());
        let t = frame_t.wrapping_mul(7) / 50;

        // The trig helpers use bits 0..16 of the angle (10 index + 6 frac).
        let angle = t as u32;
        let sin_lerp = trig::sin(angle);
        let cos_lerp = trig::cos(angle);

        let amp_x = Fixed::from_raw((*world)._field_7794 as i32);
        let amp_y = Fixed::from_raw((*world)._field_7798 as i32);

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
        let x_raw = ((viewport_w / 2 + 0x20) << 10).wrapping_sub((*runtime)._field_27c.to_raw());
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
        let field_454_raw = (*runtime)._field_454.to_raw();

        if field_454_raw == 0 {
            (*runtime)._field_458 = if (*world).field_7ea0 > 0 { 1 } else { -1 } as u32;
        }

        let game_info = (*world).game_info;
        // `game_info._field_f37c` lives in the unmapped middle of GameInfo;
        // accessed here via a raw byte offset until the field is broken out.
        let field_f37c = *((game_info as *const u8).add(0xF37C) as *const i32);
        if field_f37c != 0 || field_454_raw <= 0 {
            return;
        }

        let display = (*world).display;
        let viewport_w = (*world).viewport_pixel_width;
        let viewport_h = (*world).viewport_pixel_height;
        let palette = ((*world).scaled_frame_accum.to_raw() / 50) as u32;

        // y = field_454 * 0x46 - ((vh/2 + 0x23) << 16)
        let y_raw = field_454_raw
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
