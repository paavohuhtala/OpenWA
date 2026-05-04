//! Rust port of DisplayGfx::BlitSprite (slot 19) and DrawScaledSprite (slot 20).
//!
//! These are the high-level sprite blit entry points that resolve sprite IDs,
//! apply palette transforms, compute destination coordinates with orientation,
//! and dispatch to the low-level BitGrid blit routines.

use crate::address::va;
use crate::bitgrid::DisplayBitGrid;
use crate::bitgrid::blit::{blit_impl, blit_stippled_raw, blit_tiled_raw};
use crate::rebase::rb;
use crate::render::display::gfx::DisplayGfx;
use crate::render::display::vtable as display_vtable;
use crate::render::display::vtable::DrawScaledSpriteResult;
use crate::render::{SpriteFlags, SpriteOp};
use openwa_core::fixed::Fixed;

/// Rust port of DisplayGfx::BlitSprite (slot 19, 0x56B080).
///
/// Resolves sprite ID + animation, applies palette transforms, computes
/// destination coordinates with orientation, and dispatches to the appropriate
/// blit routine (normal, stippled, or tiled).
pub unsafe fn blit_sprite(
    this: *mut DisplayGfx,
    x: Fixed,
    y: Fixed,
    sprite: SpriteOp,
    palette: u32,
) {
    unsafe {
        let gfx = this;
        let base = this as *const u8;

        // ---------------------------------------------------------------
        // Extract sprite index and flags
        // ---------------------------------------------------------------
        let flags = sprite.flags();
        let sprite_id = sprite.index() as u32;

        if sprite_id == 0 {
            return;
        }

        // ---------------------------------------------------------------
        // Palette manipulation
        // ---------------------------------------------------------------
        let mut pal: u32 = palette;
        if flags.contains(SpriteFlags::INVERT_PALETTE) {
            pal = 0x10000u32.wrapping_sub(palette);
            if sprite_id.wrapping_sub(0x1D5) < 3 {
                // Special sprite IDs: scale by 8/18
                pal = (0x10000u32.wrapping_sub(palette).wrapping_mul(8)) / 0x12;
            }
        }
        if flags.contains(SpriteFlags::PALETTE_XFORM) {
            // Bit 25: palette transform (modular arithmetic for color cycling)
            let tmp = ((pal.wrapping_mul(0x1F) as i32)
                .wrapping_add(((pal.wrapping_mul(0x1F) as i32) >> 31) & 0x1F)
                >> 5) as u32;
            let tmp = tmp.wrapping_add(0x400) & 0xFFFF;
            pal = (tmp.wrapping_rem(0xF800)) / 2;
            if (pal & 0x400) != 0 {
                pal = (pal & !0x400) | 0x8000;
            }
        }

        // ---------------------------------------------------------------
        // Check sprite arrays — bitmap path if not in primary arrays
        // ---------------------------------------------------------------
        let arr1 = *(base.add(sprite_id as usize * 4 + 0x1008) as *const u32);
        let arr2 = *(base.add(sprite_id as usize * 4 + 0x2008) as *const u32);

        if arr1 == 0 && arr2 == 0 {
            // Bitmap sprite path — sprite is in the bitmap table at 0x3DD4.
            let bitmap_obj = (*gfx).sprite_table[sprite_id as usize];
            if bitmap_obj.is_null() {
                return;
            }

            // Get frame data and dimensions from bitmap sprite object.
            let mut sprite_w: i32 = 0;
            let mut sprite_h: i32 = 0;
            let mut rect_left: i32 = 0;
            let mut rect_top: i32 = 0;
            let mut rect_right: i32 = 0;
            let mut rect_bottom: i32 = 0;
            let frame_data = display_vtable::get_bitmap_sprite_info(
                bitmap_obj,
                pal,
                &mut sprite_w,
                &mut sprite_h,
                &mut rect_left,
                &mut rect_top,
                &mut rect_right,
                &mut rect_bottom,
            );
            if frame_data.is_null() {
                return;
            }

            let camera_x = (*gfx).camera_x;
            let camera_y = (*gfx).camera_y;
            let half_w = sprite_w / 2;
            let half_h = sprite_h / 2;
            let blit_h = rect_bottom - rect_top;
            let dst_x = (x.0 >> 16) + (camera_x - half_w) + rect_left;
            let dst_y = (y.0 >> 16) + (camera_y - half_h) + rect_top;

            if !flags.contains(SpriteFlags::TILED) {
                // Non-tiled: clipped blit at (dst_x, dst_y).
                display_vtable::blit_bitmap_clipped_native(
                    gfx, dst_x, dst_y, sprite_w, blit_h, frame_data, 2,
                );
            } else {
                // Tiled: horizontal tile across the clip rect.
                display_vtable::blit_bitmap_tiled_native(
                    gfx, dst_x, sprite_w, dst_y, blit_h, frame_data,
                );
            }
            return;
        }

        // PALETTE_X4 (sprite_flags & 0x01000000): mask palette to (pal*4 + 0x8000) & 0xFFFF.
        // Earlier ports also derived an orientation override from the overflow quadrant of
        // the unmasked product, which gave bungee worms a 90° wrong rotation. WA's
        // BlitSprite (0x56B080) discards the high bits — only the masked palette is used.
        let mut orient_local: u32 = 0x00000001; // blend=1 (ColorTable), orientation=Normal
        if flags.contains(SpriteFlags::PALETTE_X4) {
            pal = pal.wrapping_mul(4).wrapping_add(0x8000) & 0xFFFF;
        }

        // ---------------------------------------------------------------
        // Sprite frame lookup via DisplayGfx::GetSpriteFrameForBlit (slot 33).
        // ---------------------------------------------------------------
        // Resolves the sprite ID + animation value into the actual decompressed
        // frame surface plus its bounding box and full sprite-cell dimensions
        // (used below for centering and clipped blits).
        let mut out_sprite_w: i32 = 0;
        let mut out_sprite_h: i32 = 0;
        let mut out_rect_left: i32 = 0;
        let mut out_rect_top: i32 = 0;
        let mut out_rect_right: i32 = 0;
        let mut out_rect_bottom: i32 = 0;
        let mut out_anim_frac: u32 = 0;

        let mut sprite_surface = DisplayGfx::get_sprite_frame_for_blit_raw(
            gfx,
            sprite_id,
            pal,
            &mut out_sprite_w,
            &mut out_sprite_h,
            &mut out_rect_left,
            &mut out_rect_top,
            &mut out_rect_right,
            &mut out_rect_bottom,
            &mut out_anim_frac,
        );

        if sprite_surface.is_null() {
            return;
        }

        let sprite_w = out_sprite_w;
        let sprite_h = out_sprite_h;
        let rect_left = out_rect_left;
        let rect_top = out_rect_top;
        let rect_right = out_rect_right;
        let rect_bottom = out_rect_bottom;

        // Size checks
        if rect_left >= rect_right || rect_top >= rect_bottom {
            return;
        }

        let mut blit_w = rect_right - rect_left;
        let mut blit_h = rect_bottom - rect_top;

        // ---------------------------------------------------------------
        // Shadow clear (high_flags bit 22)
        // ---------------------------------------------------------------
        if flags.contains(SpriteFlags::SHADOW_CLEAR) {
            // Blit sprite to layer_2 as shadow base
            let layer2 = (*gfx).layer_2;
            blit_impl(
                layer2,
                0,
                0,
                blit_w,
                blit_h,
                sprite_surface,
                0,
                0,
                core::ptr::null(),
                0, // mode 0 = copy
            );
            // Manipulate color_add_table entry for shadow
            let color_idx = ((*gfx)._unknown_356c as usize) * 0x100;
            let table_byte = &mut (*gfx).color_add_table[color_idx];
            let saved = *table_byte;
            *table_byte = 0;

            // BitGrid__RemapPixelsThroughLut (0x4F6590) — remaps layer_2 pixels
            // through the color table.
            remap_pixels_through_shadow_lut(layer2, table_byte as *mut u8);

            *table_byte = saved;

            // Replace sprite surface with layer_2 (shadow-processed)
            sprite_surface = layer2;
        }

        // ---------------------------------------------------------------
        // Extra orientation flags from high_flags
        // ---------------------------------------------------------------
        if flags.contains(SpriteFlags::MIRROR_X) {
            orient_local |= 0x00010000;
        }
        if flags.contains(SpriteFlags::MIRROR_Y) {
            orient_local |= 0x00020000;
        }

        // ---------------------------------------------------------------
        // 16-case orientation switch for camera coordinate mapping
        // ---------------------------------------------------------------
        let camera_x = (*gfx).camera_x;
        let camera_y = (*gfx).camera_y;

        // Signed divide toward zero (matches MSVC CDQ+SUB+SAR pattern)
        let half_w = if sprite_w < 0 {
            (sprite_w + 1) / 2
        } else {
            sprite_w / 2
        };
        let half_h = if sprite_h < 0 {
            (sprite_h + 1) / 2
        } else {
            sprite_h / 2
        };

        let x_px = x.0 >> 16;
        let y_px = y.0 >> 16;

        let (dst_x, dst_y);
        let orientation_key = (orient_local >> 16) as i32;

        match orientation_key {
            1 | 10 => {
                // MirrorX
                dst_x = camera_x + half_w + x_px - rect_right;
                dst_y = camera_y - half_h + rect_top + y_px;
            }
            2 | 9 => {
                // MirrorY — X same as Normal, Y mirrored
                dst_x = camera_x - half_w + rect_left + x_px;
                dst_y = camera_y + half_h + y_px - rect_bottom;
            }
            3 | 8 => {
                // MirrorXY
                dst_x = camera_x + half_w + x_px - rect_right;
                dst_y = camera_y + half_h + y_px - rect_bottom;
            }
            4 | 15 => {
                // Rotate90 — swap axes
                dst_x = camera_x - half_h + rect_top + x_px;
                dst_y = camera_y + half_w + y_px - rect_right;
                blit_w = rect_bottom - rect_top;
                blit_h = rect_right - rect_left;
            }
            5 | 14 => {
                // Rotate90MirrorX
                dst_x = camera_x + half_h + x_px - rect_bottom;
                dst_y = camera_y + half_w + y_px - rect_right;
                blit_w = rect_bottom - rect_top;
                blit_h = rect_right - rect_left;
            }
            6 | 13 => {
                // Rotate90MirrorY
                dst_x = camera_x - half_h + rect_top + x_px;
                dst_y = camera_y - half_w + rect_left + y_px;
                blit_w = rect_bottom - rect_top;
                blit_h = rect_right - rect_left;
            }
            7 | 12 => {
                // Rotate90MirrorXY
                dst_x = camera_x + half_h + x_px - rect_bottom;
                dst_y = camera_y - half_w + rect_left + y_px;
                blit_w = rect_bottom - rect_top;
                blit_h = rect_right - rect_left;
            }
            _ => {
                // Normal (0, 11, and any other value)
                dst_x = camera_x - half_w + rect_left + x_px;
                dst_y = camera_y - half_h + rect_top + y_px;
            }
        }

        // ---------------------------------------------------------------
        // Blit dispatch based on high_flags
        // ---------------------------------------------------------------

        if blit_w <= 0 || blit_h <= 0 {
            return;
        }

        // Stippled mode (checkerboard per-pixel blit)
        if flags.intersects(SpriteFlags::STIPPLED_0 | SpriteFlags::STIPPLED_1) {
            let stipple_mode: u32 = if flags.contains(SpriteFlags::STIPPLED_1) {
                1
            } else {
                0
            };
            let parity = *(rb(va::G_STIPPLE_PARITY) as *const u32);

            display_vtable::acquire_render_lock(gfx);

            blit_stippled_raw(
                (*gfx).layer_0,
                sprite_surface,
                dst_x,
                dst_y,
                blit_w,
                blit_h,
                0,
                0,
                stipple_mode,
                parity,
            );
            return;
        }

        // Tiled mode (horizontal sprite tiling)
        if flags.contains(SpriteFlags::TILED) {
            display_vtable::acquire_render_lock(gfx);

            blit_tiled_raw(
                (*gfx).layer_0,
                sprite_surface,
                dst_x,
                dst_y,
                blit_w,
                blit_h,
                (*gfx).base.clip_x1,
                (*gfx).base.clip_x2,
                orient_local,
            );
            return;
        }

        // Determine color table pointer
        let color_table: *const u8 = if flags.contains(SpriteFlags::ADDITIVE) {
            (*gfx).color_add_table.as_ptr()
        } else if flags.contains(SpriteFlags::COLOR_BLEND) {
            (*gfx).color_blend_table.as_ptr()
        } else {
            core::ptr::null()
        };

        display_vtable::acquire_render_lock(gfx);

        // src_x=0, src_y=0 always — vtable[33] already set up the sprite surface
        blit_impl(
            (*gfx).layer_0,
            dst_x,
            dst_y,
            blit_w,
            blit_h,
            sprite_surface,
            0,
            0,
            color_table,
            orient_local,
        );
    }
}

/// Rust port of DisplayGfx::DrawScaledSprite (slot 20, 0x56B5F0).
///
/// Computes coordinates in the vtable helper, then dispatches the blit
/// via blit_impl or blit_stippled_raw depending on the result.
pub unsafe fn draw_scaled_sprite(
    this: *mut DisplayGfx,
    x: Fixed,
    y: Fixed,
    sprite: *mut DisplayBitGrid,
    src_x: i32,
    src_y: i32,
    src_w: i32,
    src_h: i32,
    flags: u32,
) {
    unsafe {
        match display_vtable::draw_scaled_sprite(
            this, x, y, sprite, src_x, src_y, src_w, src_h, flags,
        ) {
            DrawScaledSpriteResult::Blit {
                layer,
                dst_x,
                dst_y,
                width,
                height,
                sprite,
                src_x,
                src_y,
                color_table,
                blit_flags,
            } => {
                blit_impl(
                    layer,
                    dst_x,
                    dst_y,
                    width,
                    height,
                    sprite,
                    src_x,
                    src_y,
                    color_table,
                    blit_flags,
                );
            }
            DrawScaledSpriteResult::Stippled {
                layer,
                dst_x,
                dst_y,
                width,
                height,
                sprite,
                src_x,
                src_y,
                stipple_mode,
            } => {
                let parity = *(rb(va::G_STIPPLE_PARITY) as *const u32);
                blit_stippled_raw(
                    layer,
                    sprite,
                    dst_x,
                    dst_y,
                    width,
                    height,
                    src_x,
                    src_y,
                    stipple_mode,
                    parity,
                );
            }
            DrawScaledSpriteResult::Handled => {}
        }
    }
}

/// Pure Rust port of `BitGrid__RemapPixelsThroughLut` (0x4F6590).
///
/// Remaps every pixel in an 8bpp BitGrid through a 256-byte LUT.
/// The inner loop is unrolled 4x, matching the original.
unsafe fn remap_pixels_through_shadow_lut(bitgrid: *mut DisplayBitGrid, color_table: *const u8) {
    unsafe {
        let grid = &*bitgrid;
        if grid.cells_per_unit != 8 {
            return;
        }

        let data = grid.data;
        let stride = grid.row_stride as usize;
        let height = grid.height as usize;
        // The original passes stride/4 as the inner loop count (4 pixels per iteration).
        let quads_per_row = stride / 4;

        let lut = core::slice::from_raw_parts(color_table, 256);
        let mut row_ptr = data;

        for _ in 0..height {
            let mut p = row_ptr;
            for _ in 0..quads_per_row {
                *p = lut[*p as usize];
                *p.add(1) = lut[*p.add(1) as usize];
                *p.add(2) = lut[*p.add(2) as usize];
                *p.add(3) = lut[*p.add(3) as usize];
                p = p.add(4);
            }
            row_ptr = row_ptr.add(stride);
        }
    }
}
