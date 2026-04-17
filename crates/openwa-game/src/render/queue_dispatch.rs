//! `RenderDrawingQueue` (0x542350) — the per-frame render-queue dispatcher.
//!
//! This is the bridge between the producer-side `RenderQueue` (a buffer of
//! enqueued draw commands) and the consumer-side `DisplayGfx` vtable. Once
//! per frame, `GameRender_Maybe` (0x533DC0) sets up a [`ClipContext`] and
//! invokes this function with `(EAX = render_queue, stack[0] = display_gfx,
//! stack[1] = clip_ctx)`. The dispatcher then:
//!
//! 1. Sorts the entry-pointer array by each command's `layer` field
//!    (ascending; using a non-stable sort to mirror MSVC `qsort`).
//! 2. Walks the sorted list from highest index downward (so high-layer
//!    commands draw first, low-layer commands draw on top).
//! 3. Switches on `cmd[0]` (command type, 0..0xE) and dispatches to the
//!    appropriate `DisplayGfx` vtable slot, passing through one of the
//!    `RQ_*` clip / translate helpers as needed.
//!
//! The 14 command types map to slots 11..22 of [`DisplayGfxVtable`]:
//!
//! | type | producer       | slot | DisplayGfx method      |
//! |------|----------------|------|------------------------|
//! | 0    | DRAW_RECT      | 18   | `fill_rect`            |
//! | 1    | DRAW_BITMAP_GLOBAL | 20 | `draw_scaled_sprite` |
//! | 2    | DRAW_TEXTBOX_LOCAL | 20 | `draw_scaled_sprite` |
//! | 3    | (no Rust producer) | 21 | `draw_via_callback` |
//! | 4    | DRAW_SPRITE_GLOBAL | 19 | `blit_sprite`        |
//! | 5    | DRAW_SPRITE_LOCAL  | 19 | `blit_sprite`        |
//! | 6    | DRAW_SPRITE_OFFSET | 19 | `blit_sprite`        |
//! | 7    | (no Rust producer) | 12 | `draw_polyline`      |
//! | 8    | DRAW_LINE_STRIP    | 14 | `draw_line_clipped`  |
//! | 9    | DRAW_POLYGON       | 13 | `draw_line`          |
//! | 0xA  | (no Rust producer) | 15 | `draw_pixel_strip`   |
//! | 0xB  | DRAW_CROSSHAIR     | 16 | `draw_crosshair`     |
//! | 0xC  | DRAW_OUTLINED_PIXEL | 17 | `draw_outlined_pixel` |
//! | 0xD  | DRAW_TILED_BITMAP  | 11 | `draw_tiled_bitmap`  |
//! | 0xE  | (no Rust producer) | 22 | `draw_tiled_terrain` |
//!
//! ## Calling convention
//!
//! `RenderDrawingQueue` is `__usercall(EAX = *mut RenderQueue,
//! stack[0] = *mut DisplayGfx, stack[1] = *mut ClipContext)`, returning
//! with `RET 0x8`. Verified from caller `GameRender_Maybe` at 0x5340af.
//!
//! ## Helper calling conventions (verified from disassembly)
//!
//! - `RQ_ClipCoordinates` (0x542BA0) is `__thiscall(ClipContext*)` taking
//!   `(x_in, y_in, ref_z, *out_x, *out_y, *out_scale)`.
//! - `RQ_ClipCoordinatesWithRef` (0x542C70) has the same shape but uses
//!   the pivot fields `clip[2]/clip[3]`.
//! - `RQ_TranslateCoordinates` (0x542B10) is `__usercall(ECX = x_in,
//!   EAX = y_in, EDI = ClipContext*, stack[0] = *out_x, stack[1] = *out_y)`.
//!   The Ghidra decompile lost the ECX input — verified from disasm at
//!   0x542B1A (`AND ECX, 0xFFFF0000`). Same shape as `RQ_ClipCoordinates`
//!   but without the perspective scale.

use crate::render::display::gfx::DisplayGfx;
use crate::render::message::{RenderMessage, TypedRenderCmd, COMMAND_TYPE_TYPED};
use crate::render::queue::RenderQueue;
use crate::render::sprite::sprite_op::SpriteOp;
use openwa_core::fixed::Fixed;

// =============================================================================
// ClipContext — the 4-i32 anchor/pivot block GameRender_Maybe builds on stack
// =============================================================================

/// Per-frame clip / camera context, built on the stack by
/// `GameRender_Maybe` (0x533DC0) and passed as the second stack arg to
/// [`render_drawing_queue`].
///
/// The first two fields are the camera anchor in fixed-16.16 coordinates;
/// the second pair are pivot points for `RQ_ClipCoordinatesWithRef` (case 6
/// with `flags & 4`). All four are sourced from `DisplayGfx + 0x8CEC..0x8CFC`
/// at the start of `GameRender_Maybe` and adjusted in place by the
/// perspective-zoom math before being handed to the dispatcher.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ClipContext {
    /// Camera anchor X in Fixed16. Subtracted from each command's source
    /// X by the clip helpers; only the upper 16 bits matter.
    pub cam_x: Fixed,
    /// Camera anchor Y in Fixed16.
    pub cam_y: Fixed,
    /// Pivot X for the perspective-with-pivot path
    /// (`RQ_ClipCoordinatesWithRef`, case 6 with `cmd.flags & 4`).
    /// Stored as Fixed16 with the fractional bits guaranteed zero by
    /// `GameRender_Maybe`'s setup math.
    pub pivot_x: Fixed,
    /// Pivot Y — same as `pivot_x`.
    pub pivot_y: Fixed,
}

const _: () = assert!(core::mem::size_of::<ClipContext>() == 0x10);

// =============================================================================
// Pixel-aligned subtract with overflow saturation
// =============================================================================

/// Pixel-aligned subtract with overflow saturation (the inner loop body
/// shared by `RQ_ClipCoordinates` and `RQ_TranslateCoordinates`).
///
/// Both inputs are masked to their upper 16 bits via [`Fixed::floor`]
/// (truncated to pixel resolution while staying in fixed-16.16
/// representation). Returns `in_val - cam` clamped to `Fixed(i32::MIN)`
/// / `Fixed(i32::MAX)` on either-direction overflow.
///
/// `FixedDiv16_16` (0x5B3501), incidentally also used by these helpers,
/// is the same operation as [`Fixed::div`] (the `Div` impl on `Fixed`):
/// `(num << 16) / denom` with a 64-bit signed intermediate. We don't
/// need a standalone `fixed_div_16_16` helper.
#[inline]
fn clip_sub_saturate(in_val: Fixed, cam: Fixed) -> Fixed {
    let in_val = in_val.floor();
    let cam = cam.floor();
    let delta = Fixed(in_val.0.wrapping_sub(cam.0));
    if in_val.0 < 0 {
        // in_val is negative; if cam is positive and delta wrapped positive,
        // it's an underflow → saturate to MIN.
        if cam.0 > 0 && delta.0 > 0 {
            return Fixed(i32::MIN);
        }
    } else {
        // in_val is non-negative; if cam is negative and delta wrapped
        // negative, it's an overflow → saturate to MAX.
        if cam.0 < 0 && delta.0 < 0 {
            return Fixed(i32::MAX);
        }
    }
    delta
}

/// Perspective scale `0x100_0000 / projected_z` as a Fixed16 ratio.
/// Pulled out so the same expression doesn't have to be re-derived in
/// each clip helper.
#[inline]
fn perspective_scale(projected_z: i32) -> Fixed {
    // Equivalent to the WA `FixedDiv16_16` call: `(0x100_0000 << 16) / projected_z`.
    Fixed::from_raw(0x0100_0000) / Fixed::from_raw(projected_z)
}

// =============================================================================
// RQ_ClipCoordinates (0x542BA0) — perspective-scaled clip
// =============================================================================

/// `RQ_ClipCoordinates` (0x542BA0) — perspective clip with no pivot.
///
/// Returns `false` if `ref_z + 0x100_0000 < 1` (projection plane behind
/// the eye); otherwise writes `(out_x, out_y, out_scale)` and returns true.
///
/// Calling convention in WA: `__thiscall(ClipContext*)`. The Rust port
/// takes the context by reference instead.
pub fn rq_clip_coordinates(
    clip: &ClipContext,
    x_in: Fixed,
    y_in: Fixed,
    ref_z: i32,
    out_x: &mut Fixed,
    out_y: &mut Fixed,
    out_scale: &mut Fixed,
) -> bool {
    let delta_x = clip_sub_saturate(x_in, clip.cam_x);
    let delta_y = clip_sub_saturate(y_in, clip.cam_y);

    let projected_z = ref_z.wrapping_add(0x0100_0000);
    if projected_z < 1 {
        return false;
    }
    let scale = perspective_scale(projected_z);
    *out_scale = scale;
    *out_x = delta_x.mul_raw(scale);
    *out_y = delta_y.mul_raw(scale);
    true
}

// =============================================================================
// RQ_ClipCoordinatesWithRef (0x542C70) — perspective with pivot
// =============================================================================

/// `RQ_ClipCoordinatesWithRef` (0x542C70) — perspective clip relative to
/// a pivot point (read from `clip.pivot_x/pivot_y`).
///
/// The `(cam − pivot)` delta is scaled by the perspective factor and added
/// back to `pivot`, producing the projected camera position. The input
/// `(x_in, y_in)` is then subtracted from that projected position with
/// saturation.
///
/// Used by case 6 (`DRAW_SPRITE_OFFSET`) when `flags & 4` is set.
pub fn rq_clip_coordinates_with_ref(
    clip: &ClipContext,
    x_in: Fixed,
    y_in: Fixed,
    ref_z: i32,
    out_x: &mut Fixed,
    out_y: &mut Fixed,
    out_scale: &mut Fixed,
) -> bool {
    let projected_z = ref_z.wrapping_add(0x0100_0000);
    if projected_z < 1 {
        return false;
    }
    let scale = perspective_scale(projected_z);
    *out_scale = scale;

    // Project camera position relative to pivot, then subtract from input.
    let cam_x_pixel = clip.cam_x.floor();
    let cam_y_pixel = clip.cam_y.floor();

    let projected_cam_x = (cam_x_pixel - clip.pivot_x).mul_raw(scale) + clip.pivot_x;
    let projected_cam_y = (cam_y_pixel - clip.pivot_y).mul_raw(scale) + clip.pivot_y;

    *out_x = sub_saturate(x_in, projected_cam_x);
    *out_y = sub_saturate(y_in, projected_cam_y);
    true
}

/// Saturating signed subtract on `Fixed` values. Used by
/// `rq_clip_coordinates_with_ref`'s outer subtract, which operates on
/// already-projected positions where the inputs are no longer guaranteed
/// pixel-aligned, so it does NOT pre-floor like [`clip_sub_saturate`].
#[inline]
fn sub_saturate(in_val: Fixed, cam: Fixed) -> Fixed {
    let delta = Fixed(in_val.0.wrapping_sub(cam.0));
    if in_val.0 < 0 {
        if cam.0 > 0 && delta.0 > 0 {
            return Fixed(i32::MIN);
        }
    } else if cam.0 < 0 && delta.0 < 0 {
        return Fixed(i32::MAX);
    }
    delta
}

// =============================================================================
// RQ_TranslateCoordinates (0x542B10) — clip with no perspective
// =============================================================================

/// `RQ_TranslateCoordinates` (0x542B10) — same shape as
/// `rq_clip_coordinates` but without the perspective scaling. Always
/// succeeds.
///
/// In WA this is `__usercall(ECX = x_in, EAX = y_in, EDI = ClipContext*,
/// stack[0] = *out_x, stack[1] = *out_y)`. The Ghidra decompile lost the
/// ECX input (annotated `unaff_EDI` for the context and `in_EAX` for `y`,
/// silently dropping `x`); verified from the prologue disassembly at
/// 0x542B1A which masks ECX with `0xFFFF_0000`.
pub fn rq_translate_coordinates(
    clip: &ClipContext,
    x_in: Fixed,
    y_in: Fixed,
    out_x: &mut Fixed,
    out_y: &mut Fixed,
) {
    *out_x = clip_sub_saturate(x_in, clip.cam_x);
    *out_y = clip_sub_saturate(y_in, clip.cam_y);
}

// =============================================================================
// RenderDrawingQueue (0x542350) — the dispatcher
// =============================================================================

/// `RenderDrawingQueue` (0x542350) — the per-frame render-queue dispatcher.
///
/// See module-level docs for the per-case slot mapping and calling
/// convention. The Rust signature here flattens the WA usercall into a
/// plain function: a thin trampoline in `openwa-dll` captures the EAX
/// register input and forwards the two stack args.
///
/// # Safety
///
/// All three pointers must be valid for the duration of the call. The
/// render queue's `entry_count` must accurately reflect the number of
/// `entry_ptrs` slots in use, and each entry pointer must point at a
/// valid command structure whose first `u32` is a command type in `0..=0xE`
/// (commands with type `> 0xE` are silently ignored, matching WA — the
/// `JA 0x542a9e` at 0x54239f branches into the loop tail).
pub unsafe fn render_drawing_queue(
    rq: *mut RenderQueue,
    display: *mut DisplayGfx,
    clip: *mut ClipContext,
) {
    let count = (*rq).entry_count as usize;
    if count == 0 {
        return;
    }
    let entry_ptrs_base = core::ptr::addr_of_mut!((*rq).entry_ptrs) as *mut *mut u8;
    let entries: &mut [*mut u8] = core::slice::from_raw_parts_mut(entry_ptrs_base, count);

    // Sort by cmd[1] = layer field. The original uses MSVC qsort (unstable);
    // we mirror that with sort_unstable_by_key.
    entries.sort_unstable_by_key(|&p| {
        // SAFETY: entries are valid command pointers; cmd[1] is in range.
        unsafe { *(p as *const i32).add(1) }
    });

    let clip_ref: &ClipContext = &*clip;

    // Walk from the highest index downward.
    for i in (0..count).rev() {
        let cmd = entries[i] as *const u32;
        let cmd_type = *cmd;
        match cmd_type {
            0 => dispatch_case_0_fill_rect(display, clip_ref, cmd),
            1 => dispatch_case_1_bitmap_global(display, cmd),
            2 => dispatch_case_2_textbox_local(display, clip_ref, cmd),
            3 => dispatch_case_3_via_callback(display, clip_ref, cmd),
            4 => dispatch_case_4_sprite_global(display, cmd),
            5 => dispatch_case_5_sprite_local(display, clip_ref, cmd),
            6 => dispatch_case_6_sprite_offset(display, clip_ref, cmd),
            7 => dispatch_case_7_polyline(display, cmd),
            8 => dispatch_case_8_line_strip(display, clip_ref, cmd),
            9 => dispatch_case_9_polygon(display, clip_ref, cmd),
            0xA => dispatch_case_a_pixel_strip(display, clip_ref, cmd),
            0xB => dispatch_case_b_crosshair(display, clip_ref, cmd),
            0xC => dispatch_case_c_outlined_pixel(display, clip_ref, cmd),
            0xD => dispatch_case_d_tiled_bitmap(display, clip_ref, cmd),
            0xE => dispatch_case_e_tiled_terrain(display, clip_ref, cmd),
            COMMAND_TYPE_TYPED => {
                let typed = &(*(cmd as *const TypedRenderCmd)).message;
                dispatch_typed(display, clip_ref, typed);
            }
            _ => { /* unknown command type — silently skipped, matching WA */ }
        }
    }
}

// =============================================================================
// Per-case dispatchers
// =============================================================================
//
// Each case body mirrors the corresponding `case N:` arm of WA's
// `RenderDrawingQueue` (0x542350). Field offsets are the `puVar1[N]`
// indices from the decompile (each step = 4 bytes).
//
// Vtable dispatch goes through the auto-generated `DisplayGfx::*_raw`
// methods (from `bind_DisplayGfxVtable!(DisplayGfx, base.vtable)`),
// which take a raw `*mut DisplayGfx` to avoid creating `&mut self` —
// see the noalias rule in CLAUDE.md.

#[inline]
unsafe fn read_field(cmd: *const u32, idx: usize) -> i32 {
    *(cmd.add(idx) as *const i32)
}

#[inline]
unsafe fn read_fixed(cmd: *const u32, idx: usize) -> Fixed {
    Fixed::from_raw(*(cmd.add(idx) as *const i32))
}

// ---- case 0: DRAW_RECT → slot 18 (fill_rect) -------------------------------
//
//     iVar4 = RQ_ClipCoordinates(this, cmd[3], cmd[4], cmd[7], &local_18, &param_1, ..);
//     if !iVar4 break;
//     iVar4 = RQ_ClipCoordinates(this, cmd[5], cmd[6], cmd[7], &local_10, &local_c, ..);
//     if !iVar4 break;
//     // x/y MIN-corner clamps to keep pixel-perfect rect edges
//     vtable[18](this, x1>>16, y1>>16, x2>>16, y2>>16, cmd[2]);
unsafe fn dispatch_case_0_fill_rect(display: *mut DisplayGfx, clip: &ClipContext, cmd: *const u32) {
    let mut x1 = Fixed::ZERO;
    let mut y1 = Fixed::ZERO;
    let mut x2 = Fixed::ZERO;
    let mut y2 = Fixed::ZERO;
    let mut scale = Fixed::ZERO;
    if !rq_clip_coordinates(
        clip,
        read_fixed(cmd, 3),
        read_fixed(cmd, 4),
        read_field(cmd, 7),
        &mut x1,
        &mut y1,
        &mut scale,
    ) {
        return;
    }
    if !rq_clip_coordinates(
        clip,
        read_fixed(cmd, 5),
        read_fixed(cmd, 6),
        read_field(cmd, 7),
        &mut x2,
        &mut y2,
        &mut scale,
    ) {
        return;
    }

    // Pixel-perfect MIN/MAX rect-edge clamping when the source is at the
    // sentinel anchor values: cmd[3]/cmd[4] == -0x80000000 → clamp to MIN,
    // cmd[5]/cmd[6] == 0x7FFF0000 → clamp to MAX. WA uses these to draw
    // "infinite" rectangles in screen space.
    if read_field(cmd, 3) == i32::MIN {
        x1 = Fixed(i32::MIN);
    }
    if read_field(cmd, 4) == i32::MIN {
        y1 = Fixed(i32::MIN);
    }
    if read_field(cmd, 5) == 0x7FFF_0000 {
        x2 = Fixed(i32::MAX);
    }
    if read_field(cmd, 6) == 0x7FFF_0000 {
        y2 = Fixed(i32::MAX);
    }

    let color = *(cmd.add(2));
    DisplayGfx::fill_rect_raw(
        display,
        x1.to_int(),
        y1.to_int(),
        x2.to_int(),
        y2.to_int(),
        color,
    );
}

// ---- case 1: DRAW_BITMAP_GLOBAL → slot 20 (draw_scaled_sprite) -------------
//
//     vtable[20](this, cmd[2], cmd[3], cmd[4], cmd[5], cmd[6], cmd[7], cmd[8], cmd[9]);
//
// Pure passthrough — global bitmaps draw at the supplied world coords.
unsafe fn dispatch_case_1_bitmap_global(display: *mut DisplayGfx, cmd: *const u32) {
    use crate::bitgrid::DisplayBitGrid;
    let x = read_fixed(cmd, 2);
    let y = read_fixed(cmd, 3);
    let sprite = read_field(cmd, 4) as *mut DisplayBitGrid;
    let src_x = read_field(cmd, 5);
    let src_y = read_field(cmd, 6);
    let src_w = read_field(cmd, 7);
    let src_h = read_field(cmd, 8);
    let flags = *(cmd.add(9));
    DisplayGfx::draw_scaled_sprite_raw(display, x, y, sprite, src_x, src_y, src_w, src_h, flags);
}

// ---- case 2: DRAW_TEXTBOX_LOCAL → slot 20 (draw_scaled_sprite) -------------
//
// `cmd[2] == 0` selects the simple variant (one clip call); otherwise the
// box has separate top-edge / bottom-edge clip targets and a flag byte at
// `cmd+8` controlling whether the result clamps to the second clip pair.
unsafe fn dispatch_case_2_textbox_local(
    display: *mut DisplayGfx,
    clip: &ClipContext,
    cmd: *const u32,
) {
    use crate::bitgrid::DisplayBitGrid;
    let mode = *(cmd.add(2));
    let mut x1 = Fixed::ZERO;
    let mut y1 = Fixed::ZERO;
    let mut scale = Fixed::ZERO;
    if mode == 0 {
        if !rq_clip_coordinates(
            clip,
            read_fixed(cmd, 3),
            read_fixed(cmd, 4),
            read_field(cmd, 5),
            &mut x1,
            &mut y1,
            &mut scale,
        ) {
            return;
        }
    } else {
        if !rq_clip_coordinates(
            clip,
            read_fixed(cmd, 3),
            read_fixed(cmd, 4),
            read_field(cmd, 5),
            &mut x1,
            &mut y1,
            &mut scale,
        ) {
            return;
        }
        let mut x2 = Fixed::ZERO;
        let mut y2 = Fixed::ZERO;
        if !rq_clip_coordinates(
            clip,
            read_fixed(cmd, 3),
            read_fixed(cmd, 4),
            read_field(cmd, 6),
            &mut x2,
            &mut y2,
            &mut scale,
        ) {
            return;
        }
        let flag_byte = *(cmd.add(2)) as u8;
        if flag_byte & 1 != 0 {
            x1 = x2;
        }
        if flag_byte & 2 != 0 {
            y1 = y2;
        }
    }
    let sprite = read_field(cmd, 7) as *mut DisplayBitGrid;
    let src_x = read_field(cmd, 8);
    let src_y = read_field(cmd, 9);
    let src_w = read_field(cmd, 10);
    let src_h = read_field(cmd, 11);
    let flags = *(cmd.add(12));
    DisplayGfx::draw_scaled_sprite_raw(display, x1, y1, sprite, src_x, src_y, src_w, src_h, flags);
}

// ---- case 3 → slot 21 (draw_via_callback) ----------------------------------
//
//     iVar4 = RQ_ClipCoordinates(this, cmd[2], cmd[3], cmd[4], &x, &y, ..);
//     if !iVar4 break;
//     vtable[21](this, x, y, cmd[6], cmd[7], cmd[8]);
unsafe fn dispatch_case_3_via_callback(
    display: *mut DisplayGfx,
    clip: &ClipContext,
    cmd: *const u32,
) {
    let mut x = Fixed::ZERO;
    let mut y = Fixed::ZERO;
    let mut scale = Fixed::ZERO;
    if !rq_clip_coordinates(
        clip,
        read_fixed(cmd, 2),
        read_fixed(cmd, 3),
        read_field(cmd, 4),
        &mut x,
        &mut y,
        &mut scale,
    ) {
        return;
    }
    let obj = read_field(cmd, 6) as *mut u8;
    let p5 = *(cmd.add(7));
    let p6 = *(cmd.add(8));
    DisplayGfx::draw_via_callback_raw(display, x, y, obj, p5, p6);
}

// ---- case 4: DRAW_SPRITE_GLOBAL → slot 19 (blit_sprite) --------------------
//
//     vtable[19](this, cmd[2], cmd[3], cmd[4], cmd[5]);
unsafe fn dispatch_case_4_sprite_global(display: *mut DisplayGfx, cmd: *const u32) {
    let x = read_fixed(cmd, 2);
    let y = read_fixed(cmd, 3);
    let sprite = SpriteOp(*(cmd.add(4)));
    let palette = *(cmd.add(5));
    DisplayGfx::blit_sprite_raw(display, x, y, sprite, palette);
}

// ---- case 5: DRAW_SPRITE_LOCAL → slot 19 (blit_sprite) ---------------------
//
//     RQ_TranslateCoordinates(clip, cmd[2], cmd[3], &x, &y);
//     vtable[19](this, x, y, cmd[4], cmd[5]);
unsafe fn dispatch_case_5_sprite_local(
    display: *mut DisplayGfx,
    clip: &ClipContext,
    cmd: *const u32,
) {
    let mut x = Fixed::ZERO;
    let mut y = Fixed::ZERO;
    rq_translate_coordinates(clip, read_fixed(cmd, 2), read_fixed(cmd, 3), &mut x, &mut y);
    let sprite = SpriteOp(*(cmd.add(4)));
    let palette = *(cmd.add(5));
    DisplayGfx::blit_sprite_raw(display, x, y, sprite, palette);
}

// ---- case 6: DRAW_SPRITE_OFFSET → slot 19 (blit_sprite) --------------------
//
// `cmd[2]` is a flag byte: bit 0/1 are top/bottom-edge clamps, bit 2 selects
// the perspective-with-pivot helper instead of the plain perspective clip.
// `cmd[2] == 0` selects the simple translate-only path used for purely
// screen-space offset sprites.
unsafe fn dispatch_case_6_sprite_offset(
    display: *mut DisplayGfx,
    clip: &ClipContext,
    cmd: *const u32,
) {
    let flags = *(cmd.add(2));

    let mut x = Fixed::ZERO;
    let mut y = Fixed::ZERO;
    let mut scale = Fixed::ZERO;

    if flags == 0 {
        // Simple translate path (no perspective).
        rq_translate_coordinates(clip, read_fixed(cmd, 3), read_fixed(cmd, 4), &mut x, &mut y);
    } else {
        // Two clip calls (top + bottom Y) with optional MIN/MAX clamps.
        let use_pivot = (flags & 4) != 0;
        let clip_fn: fn(
            &ClipContext,
            Fixed,
            Fixed,
            i32,
            &mut Fixed,
            &mut Fixed,
            &mut Fixed,
        ) -> bool = if use_pivot {
            rq_clip_coordinates_with_ref
        } else {
            rq_clip_coordinates
        };
        if !clip_fn(
            clip,
            read_fixed(cmd, 3),
            read_fixed(cmd, 4),
            read_field(cmd, 5),
            &mut x,
            &mut y,
            &mut scale,
        ) {
            return;
        }
        let mut x2 = Fixed::ZERO;
        let mut y2 = Fixed::ZERO;
        if !clip_fn(
            clip,
            read_fixed(cmd, 3),
            read_fixed(cmd, 4),
            read_field(cmd, 6),
            &mut x2,
            &mut y2,
            &mut scale,
        ) {
            return;
        }
        let flag_byte = *(cmd.add(2)) as u8;
        if flag_byte & 1 != 0 {
            x = x2;
        }
        if flag_byte & 2 != 0 {
            y = y2;
        }
    }

    let sprite = SpriteOp(*(cmd.add(7)));
    let palette = *(cmd.add(8));
    DisplayGfx::blit_sprite_raw(display, x, y, sprite, palette);
}

// ---- case 7 → slot 12 (draw_polyline) --------------------------------------
//
//     vtable[12](this, &cmd[4], cmd[2], cmd[3]);
//
// `cmd[2]` is the vertex count, `cmd[3]` is the color, vertex data
// follows starting at `cmd[4]`.
unsafe fn dispatch_case_7_polyline(display: *mut DisplayGfx, cmd: *const u32) {
    let points = cmd.add(4) as *mut i32;
    let count = read_field(cmd, 2);
    let color = *(cmd.add(3));
    DisplayGfx::draw_polyline_raw(display, points, count, color);
}

// ---- case 8: DRAW_LINE_STRIP → slot 14 (draw_line_clipped) -----------------
//
// First vertex at `cmd[4..6]` plus `cmd[6]` (z), then iterate
// `(count - 1)` more vertices and call slot 14 for each segment.
// `cmd[3]` is the color shared by all segments.
unsafe fn dispatch_case_8_line_strip(
    display: *mut DisplayGfx,
    clip: &ClipContext,
    cmd: *const u32,
) {
    let mut x1 = Fixed::ZERO;
    let mut y1 = Fixed::ZERO;
    let mut scale = Fixed::ZERO;
    if !rq_clip_coordinates(
        clip,
        read_fixed(cmd, 4),
        read_fixed(cmd, 5),
        read_field(cmd, 6),
        &mut x1,
        &mut y1,
        &mut scale,
    ) {
        return;
    }
    let count = read_field(cmd, 2);
    let color = *(cmd.add(3));
    if count <= 1 {
        return;
    }
    // Vertex N is at cmd[8 + (N-1)*3 .. 8 + (N-1)*3 + 3], a 3-tuple of (x, y, z).
    // Walk N = 1..count.
    let mut vert = cmd.add(8) as *const u32;
    for _ in 1..count {
        let mut x2 = Fixed::ZERO;
        let mut y2 = Fixed::ZERO;
        if !rq_clip_coordinates(
            clip,
            Fixed::from_raw(*(vert.sub(1)) as i32),
            Fixed::from_raw(*vert as i32),
            *(vert.add(1)) as i32,
            &mut x2,
            &mut y2,
            &mut scale,
        ) {
            break;
        }
        DisplayGfx::draw_line_clipped_raw(display, x1, y1, x2, y2, color);
        x1 = x2;
        y1 = y2;
        vert = vert.add(3);
    }
}

// ---- case 9: DRAW_POLYGON → slot 13 (draw_line) ----------------------------
//
// Same shape as case 8 but with two color params (cmd[3], cmd[4]) and the
// first vertex starts at `cmd[5..8]`. Per-segment vertex stride is 3 u32s.
unsafe fn dispatch_case_9_polygon(display: *mut DisplayGfx, clip: &ClipContext, cmd: *const u32) {
    let mut x1 = Fixed::ZERO;
    let mut y1 = Fixed::ZERO;
    let mut scale = Fixed::ZERO;
    if !rq_clip_coordinates(
        clip,
        read_fixed(cmd, 5),
        read_fixed(cmd, 6),
        read_field(cmd, 7),
        &mut x1,
        &mut y1,
        &mut scale,
    ) {
        return;
    }
    let count = read_field(cmd, 2);
    let color1 = *(cmd.add(3));
    let color2 = *(cmd.add(4));
    if count <= 1 {
        return;
    }
    let mut vert = cmd.add(9) as *const u32;
    for _ in 1..count {
        let mut x2 = Fixed::ZERO;
        let mut y2 = Fixed::ZERO;
        if !rq_clip_coordinates(
            clip,
            Fixed::from_raw(*(vert.sub(1)) as i32),
            Fixed::from_raw(*vert as i32),
            *(vert.add(1)) as i32,
            &mut x2,
            &mut y2,
            &mut scale,
        ) {
            break;
        }
        DisplayGfx::draw_line_raw(display, x1, y1, x2, y2, color1, color2);
        x1 = x2;
        y1 = y2;
        vert = vert.add(3);
    }
}

// ---- case 0xA → slot 15 (draw_pixel_strip) ---------------------------------
//
//     RQ_TranslateCoordinates(clip, cmd[2], cmd[3], &x, &y);
//     // The translate's `clip_sub_saturate` floor()s its inputs, so its
//     // output drops the fractional 16 bits of cmd[2]/cmd[3]. The dispatcher
//     // re-adds those fractional bits back to recover pixel-fractional precision:
//     x += cmd[2] & 0xFFFF;
//     y += cmd[3] & 0xFFFF;
//     vtable[15](this, x, y, cmd[4], cmd[5], cmd[6], cmd[7]);
unsafe fn dispatch_case_a_pixel_strip(
    display: *mut DisplayGfx,
    clip: &ClipContext,
    cmd: *const u32,
) {
    let mut x = Fixed::ZERO;
    let mut y = Fixed::ZERO;
    rq_translate_coordinates(clip, read_fixed(cmd, 2), read_fixed(cmd, 3), &mut x, &mut y);
    let cmd_x = *(cmd.add(2));
    let cmd_y = *(cmd.add(3));
    x += Fixed::from_raw((cmd_x & 0xFFFF) as i32);
    y += Fixed::from_raw((cmd_y & 0xFFFF) as i32);
    let dx = read_fixed(cmd, 4);
    let dy = read_fixed(cmd, 5);
    let count = read_field(cmd, 6);
    let color = *(cmd.add(7));
    DisplayGfx::draw_pixel_strip_raw(display, x, y, dx, dy, count, color);
}

// ---- case 0xB: DRAW_CROSSHAIR → slot 16 (draw_crosshair) -------------------
//
//     iVar4 = RQ_ClipCoordinates(this, cmd[4], cmd[5], cmd[6], &x, &y, ..);
//     if !iVar4 break;
//     vtable[16](this, x>>16, y>>16, cmd[2], cmd[3]);
unsafe fn dispatch_case_b_crosshair(display: *mut DisplayGfx, clip: &ClipContext, cmd: *const u32) {
    let mut x = Fixed::ZERO;
    let mut y = Fixed::ZERO;
    let mut scale = Fixed::ZERO;
    if !rq_clip_coordinates(
        clip,
        read_fixed(cmd, 4),
        read_fixed(cmd, 5),
        read_field(cmd, 6),
        &mut x,
        &mut y,
        &mut scale,
    ) {
        return;
    }
    let color_fg = *(cmd.add(2));
    let color_bg = *(cmd.add(3));
    DisplayGfx::draw_crosshair_raw(display, x.to_int(), y.to_int(), color_fg, color_bg);
}

// ---- case 0xC → slot 17 (draw_outlined_pixel) ------------------------------
//
//     iVar4 = RQ_ClipCoordinates(this, cmd[4], cmd[5], cmd[6], &x, &y, ..);
//     if !iVar4 break;
//     vtable[17](this, x>>16, y>>16, cmd[2], cmd[3]);
unsafe fn dispatch_case_c_outlined_pixel(
    display: *mut DisplayGfx,
    clip: &ClipContext,
    cmd: *const u32,
) {
    let mut x = Fixed::ZERO;
    let mut y = Fixed::ZERO;
    let mut scale = Fixed::ZERO;
    if !rq_clip_coordinates(
        clip,
        read_fixed(cmd, 4),
        read_fixed(cmd, 5),
        read_field(cmd, 6),
        &mut x,
        &mut y,
        &mut scale,
    ) {
        return;
    }
    let color_fg = *(cmd.add(2));
    let color_bg = read_field(cmd, 3);
    DisplayGfx::draw_outlined_pixel_raw(display, x.to_int(), y.to_int(), color_fg, color_bg);
}

// ---- case 0xD: DRAW_TILED_BITMAP → slot 11 (draw_tiled_bitmap) -------------
//
//     iVar4 = RQ_ClipCoordinates(this, 0, cmd[2], cmd[3], &x, &y, ..);
//     if !iVar4 break;
//     // Bit 0 of the flag byte at cmd[5] forces dest_x to 0 instead of x>>16.
//     vtable[11](this, (flag ? 0 : x>>16), y>>16, cmd[4]);
unsafe fn dispatch_case_d_tiled_bitmap(
    display: *mut DisplayGfx,
    clip: &ClipContext,
    cmd: *const u32,
) {
    use crate::render::display::vtable::TiledBitmapSource;
    let mut x = Fixed::ZERO;
    let mut y = Fixed::ZERO;
    let mut scale = Fixed::ZERO;
    if !rq_clip_coordinates(
        clip,
        Fixed::ZERO,
        read_fixed(cmd, 2),
        read_field(cmd, 3),
        &mut x,
        &mut y,
        &mut scale,
    ) {
        return;
    }
    let flag_byte = *(cmd.add(5)) as u8;
    let dest_x = if flag_byte != 0 { 0 } else { x.to_int() };
    let source = read_field(cmd, 4) as *const TiledBitmapSource;
    DisplayGfx::draw_tiled_bitmap_raw(display, dest_x, y.to_int(), source);
}

// ---- case 0xE → slot 22 (draw_tiled_terrain) -------------------------------
//
// Same shape as case 2: `cmd[2] == 0` selects the one-clip variant; non-zero
// runs the two-clip path with the same flag-byte clamps.
unsafe fn dispatch_case_e_tiled_terrain(
    display: *mut DisplayGfx,
    clip: &ClipContext,
    cmd: *const u32,
) {
    let mode = *(cmd.add(2));
    let mut x = Fixed::ZERO;
    let mut y = Fixed::ZERO;
    let mut scale = Fixed::ZERO;
    if mode == 0 {
        if !rq_clip_coordinates(
            clip,
            read_fixed(cmd, 3),
            read_fixed(cmd, 4),
            read_field(cmd, 5),
            &mut x,
            &mut y,
            &mut scale,
        ) {
            return;
        }
    } else {
        if !rq_clip_coordinates(
            clip,
            read_fixed(cmd, 3),
            read_fixed(cmd, 4),
            read_field(cmd, 5),
            &mut x,
            &mut y,
            &mut scale,
        ) {
            return;
        }
        let mut x2 = Fixed::ZERO;
        let mut y2 = Fixed::ZERO;
        if !rq_clip_coordinates(
            clip,
            read_fixed(cmd, 3),
            read_fixed(cmd, 4),
            read_field(cmd, 6),
            &mut x2,
            &mut y2,
            &mut scale,
        ) {
            return;
        }
        let flag_byte = *(cmd.add(2)) as u8;
        if flag_byte & 1 != 0 {
            x = x2;
        }
        if flag_byte & 2 != 0 {
            y = y2;
        }
    }
    let count = read_field(cmd, 7);
    let flags = *(cmd.add(8));
    DisplayGfx::draw_tiled_terrain_raw(display, x, y, count, flags);
}

// =============================================================================
// Typed message dispatcher
// =============================================================================

/// Dispatch a [`RenderMessage`] to the appropriate `DisplayGfx` vtable method.
unsafe fn dispatch_typed(display: *mut DisplayGfx, clip: &ClipContext, msg: &RenderMessage) {
    match *msg {
        RenderMessage::Sprite {
            local,
            x,
            y,
            sprite,
            palette,
        } => {
            if local {
                let mut out_x = Fixed::ZERO;
                let mut out_y = Fixed::ZERO;
                rq_translate_coordinates(clip, x, y, &mut out_x, &mut out_y);
                DisplayGfx::blit_sprite_raw(display, out_x, out_y, sprite, palette);
            } else {
                DisplayGfx::blit_sprite_raw(display, x, y, sprite, palette);
            }
        }

        RenderMessage::FillRect {
            color,
            x1,
            y1,
            x2,
            y2,
            ref_z,
        } => {
            let mut ox1 = Fixed::ZERO;
            let mut oy1 = Fixed::ZERO;
            let mut ox2 = Fixed::ZERO;
            let mut oy2 = Fixed::ZERO;
            let mut scale = Fixed::ZERO;
            if !rq_clip_coordinates(clip, x1, y1, ref_z, &mut ox1, &mut oy1, &mut scale) {
                return;
            }
            if !rq_clip_coordinates(clip, x2, y2, ref_z, &mut ox2, &mut oy2, &mut scale) {
                return;
            }
            // Sentinel clamps for "infinite" screen-space rectangles.
            if x1.0 == i32::MIN {
                ox1 = Fixed(i32::MIN);
            }
            if y1.0 == i32::MIN {
                oy1 = Fixed(i32::MIN);
            }
            if x2.0 == 0x7FFF_0000 {
                ox2 = Fixed(i32::MAX);
            }
            if y2.0 == 0x7FFF_0000 {
                oy2 = Fixed(i32::MAX);
            }
            DisplayGfx::fill_rect_raw(
                display,
                ox1.to_int(),
                oy1.to_int(),
                ox2.to_int(),
                oy2.to_int(),
                color,
            );
        }

        RenderMessage::Crosshair {
            color_fg,
            color_bg,
            x,
            y,
        } => {
            let mut ox = Fixed::ZERO;
            let mut oy = Fixed::ZERO;
            let mut scale = Fixed::ZERO;
            if !rq_clip_coordinates(clip, x, y, 0, &mut ox, &mut oy, &mut scale) {
                return;
            }
            DisplayGfx::draw_crosshair_raw(display, ox.to_int(), oy.to_int(), color_fg, color_bg);
        }

        RenderMessage::TiledBitmap {
            x,
            y,
            source,
            flags,
        } => {
            let mut ox = Fixed::ZERO;
            let mut oy = Fixed::ZERO;
            let mut scale = Fixed::ZERO;
            if !rq_clip_coordinates(clip, Fixed::ZERO, x, y.0, &mut ox, &mut oy, &mut scale) {
                return;
            }
            let dest_x = if flags & 1 != 0 { 0 } else { ox.to_int() };
            DisplayGfx::draw_tiled_bitmap_raw(display, dest_x, oy.to_int(), source);
        }

        RenderMessage::SpriteOffset {
            flags,
            x,
            y,
            ref_z_2,
            sprite,
            palette,
        } => {
            let mut ox = Fixed::ZERO;
            let mut oy = Fixed::ZERO;
            let mut scale = Fixed::ZERO;

            if flags == 0 {
                rq_translate_coordinates(clip, x, y, &mut ox, &mut oy);
            } else {
                let use_pivot = (flags & 4) != 0;
                let clip_fn: fn(
                    &ClipContext,
                    Fixed,
                    Fixed,
                    i32,
                    &mut Fixed,
                    &mut Fixed,
                    &mut Fixed,
                ) -> bool = if use_pivot {
                    rq_clip_coordinates_with_ref
                } else {
                    rq_clip_coordinates
                };
                // ref_z (first Z reference) is always 0.
                if !clip_fn(clip, x, y, 0, &mut ox, &mut oy, &mut scale) {
                    return;
                }
                let mut ox2 = Fixed::ZERO;
                let mut oy2 = Fixed::ZERO;
                if !clip_fn(clip, x, y, ref_z_2, &mut ox2, &mut oy2, &mut scale) {
                    return;
                }
                if flags & 1 != 0 {
                    ox = ox2;
                }
                if flags & 2 != 0 {
                    oy = oy2;
                }
            }

            DisplayGfx::blit_sprite_raw(display, ox, oy, sprite, palette);
        }

        RenderMessage::BitmapGlobal {
            x,
            y,
            bitmap,
            src_y,
            src_w,
            src_h,
            flags,
        } => {
            // src_x is always 0 in all known producers.
            DisplayGfx::draw_scaled_sprite_raw(
                display, x, y, bitmap, 0, src_y, src_w, src_h, flags,
            );
        }

        RenderMessage::TextboxLocal {
            x,
            y,
            bitmap,
            src_w,
            src_h,
            flags,
        } => {
            // mode is always 0, so simple one-clip path.
            // ref_z is always 0.
            let mut ox = Fixed::ZERO;
            let mut oy = Fixed::ZERO;
            let mut scale = Fixed::ZERO;
            if !rq_clip_coordinates(clip, x, y, 0, &mut ox, &mut oy, &mut scale) {
                return;
            }
            // src_x and src_y are always 0.
            DisplayGfx::draw_scaled_sprite_raw(display, ox, oy, bitmap, 0, 0, src_w, src_h, flags);
        }

        RenderMessage::LineStrip {
            count,
            color,
            vertices,
        } => {
            if count <= 1 {
                return;
            }
            let verts = core::slice::from_raw_parts(vertices, count as usize);
            let mut x1 = Fixed::ZERO;
            let mut y1 = Fixed::ZERO;
            let mut scale = Fixed::ZERO;
            if !rq_clip_coordinates(
                clip,
                Fixed::from_raw(verts[0][0]),
                Fixed::from_raw(verts[0][1]),
                verts[0][2],
                &mut x1,
                &mut y1,
                &mut scale,
            ) {
                return;
            }
            for v in &verts[1..] {
                let mut x2 = Fixed::ZERO;
                let mut y2 = Fixed::ZERO;
                if !rq_clip_coordinates(
                    clip,
                    Fixed::from_raw(v[0]),
                    Fixed::from_raw(v[1]),
                    v[2],
                    &mut x2,
                    &mut y2,
                    &mut scale,
                ) {
                    break;
                }
                DisplayGfx::draw_line_clipped_raw(display, x1, y1, x2, y2, color);
                x1 = x2;
                y1 = y2;
            }
        }

        RenderMessage::Polygon {
            count,
            color1,
            color2,
            vertices,
        } => {
            if count <= 1 {
                return;
            }
            let verts = core::slice::from_raw_parts(vertices, count as usize);
            let mut x1 = Fixed::ZERO;
            let mut y1 = Fixed::ZERO;
            let mut scale = Fixed::ZERO;
            if !rq_clip_coordinates(
                clip,
                Fixed::from_raw(verts[0][0]),
                Fixed::from_raw(verts[0][1]),
                verts[0][2],
                &mut x1,
                &mut y1,
                &mut scale,
            ) {
                return;
            }
            for v in &verts[1..] {
                let mut x2 = Fixed::ZERO;
                let mut y2 = Fixed::ZERO;
                if !rq_clip_coordinates(
                    clip,
                    Fixed::from_raw(v[0]),
                    Fixed::from_raw(v[1]),
                    v[2],
                    &mut x2,
                    &mut y2,
                    &mut scale,
                ) {
                    break;
                }
                DisplayGfx::draw_line_raw(display, x1, y1, x2, y2, color1, color2);
                x1 = x2;
                y1 = y2;
            }
        }
    }
}

// =============================================================================
// Unit tests for the helpers
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn zero_clip() -> ClipContext {
        ClipContext {
            cam_x: Fixed::ZERO,
            cam_y: Fixed::ZERO,
            pivot_x: Fixed::ZERO,
            pivot_y: Fixed::ZERO,
        }
    }

    #[test]
    fn perspective_scale_at_zero_z() {
        // 0x100_0000 / 0x100_0000 = 0x10000 (Fixed16 1.0)
        assert_eq!(perspective_scale(0x0100_0000), Fixed::ONE);
    }

    #[test]
    fn clip_sub_saturate_no_overflow() {
        // 5.0 - 2.0 = 3.0 in fixed16
        assert_eq!(
            clip_sub_saturate(Fixed::from_int(5), Fixed::from_int(2)),
            Fixed::from_int(3)
        );
    }

    #[test]
    fn clip_sub_saturate_underflow_clamps_to_min() {
        // x_in negative, cam positive, delta wraps positive → MIN
        assert_eq!(
            clip_sub_saturate(Fixed(i32::MIN), Fixed::from_raw(0x4000_0000)),
            Fixed(i32::MIN)
        );
    }

    #[test]
    fn clip_sub_saturate_overflow_clamps_to_max() {
        // x_in positive, cam negative, delta wraps negative → MAX
        assert_eq!(
            clip_sub_saturate(
                Fixed::from_raw(0x4000_0000),
                Fixed::from_raw(i32::MIN + 0x10000)
            ),
            Fixed(i32::MAX)
        );
    }

    #[test]
    fn clip_coordinates_at_z_zero_is_identity() {
        let clip = zero_clip();
        let mut ox = Fixed::ZERO;
        let mut oy = Fixed::ZERO;
        let mut scale = Fixed::ZERO;
        let ok = rq_clip_coordinates(
            &clip,
            Fixed::from_int(5),
            Fixed::from_int(7),
            0,
            &mut ox,
            &mut oy,
            &mut scale,
        );
        assert!(ok);
        // scale = 0x100_0000 / 0x100_0000 = 0x10000 (1.0 in fixed16)
        assert_eq!(scale, Fixed::ONE);
        assert_eq!(ox, Fixed::from_int(5));
        assert_eq!(oy, Fixed::from_int(7));
    }

    #[test]
    fn clip_coordinates_negative_z_returns_false() {
        let clip = zero_clip();
        let mut ox = Fixed::ZERO;
        let mut oy = Fixed::ZERO;
        let mut scale = Fixed::ZERO;
        let ok = rq_clip_coordinates(
            &clip,
            Fixed::from_int(5),
            Fixed::from_int(7),
            -0x100_0001,
            &mut ox,
            &mut oy,
            &mut scale,
        );
        assert!(!ok);
    }

    #[test]
    fn translate_coordinates_camera_offset() {
        let clip = ClipContext {
            cam_x: Fixed::from_int(10),
            cam_y: Fixed::from_int(20),
            pivot_x: Fixed::ZERO,
            pivot_y: Fixed::ZERO,
        };
        let mut ox = Fixed::ZERO;
        let mut oy = Fixed::ZERO;
        rq_translate_coordinates(
            &clip,
            Fixed::from_int(30),
            Fixed::from_int(50),
            &mut ox,
            &mut oy,
        );
        assert_eq!(ox, Fixed::from_int(20));
        assert_eq!(oy, Fixed::from_int(30));
    }
}
