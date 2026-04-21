//! BitGrid blit operations — sprite blitting on raw BitGrid pointers.
//!
//! These functions bridge raw `*mut DisplayBitGrid` pointers to the safe
//! `PixelGrid`/`BlitSource` abstractions in `render::display::sprite_blit`.

use crate::bitgrid::DisplayBitGrid;
use openwa_core::pixel_grid::PixelGridMut;
use openwa_core::sprite::{
    BlitBlend, BlitOrientation, BlitSource, blit_sprite_rect, blit_stippled, blit_tiled,
};

/// Rust implementation of the core sprite blit (BitGrid__BlitSpriteRect, 0x4F6910).
///
/// Handles all blend modes for both 8bpp and 1-bit surfaces:
/// - 8bpp modes 0-3: via `blit_sprite_rect` (orientation-aware, PixelGrid wrapper)
/// - 1-bit byte-aligned fast path: memcpy/OR/nop for modes 0-3 with Normal orientation
/// - Generic per-pixel fallback: handles all remaining cases (unaligned 1-bit, modes 4-5,
///   mixed cpp, etc.) via `blit_generic_perpixel`
pub unsafe fn blit_impl(
    dst: *mut DisplayBitGrid,
    dst_x: i32,
    dst_y: i32,
    width: i32,
    height: i32,
    src: *mut DisplayBitGrid,
    src_x: i32,
    src_y: i32,
    color_table: *const u8,
    flags: u32,
) -> u32 {
    unsafe {
        if width == 0 || height == 0 {
            return 0;
        }

        let blend_mode = flags & 0xFFFF;
        let src_cpp = (*src).cells_per_unit;
        let dst_cpp = (*dst).cells_per_unit;

        // Fast path: 8bpp surfaces with blend modes 0-3
        if dst_cpp == 8 && src_cpp == 8 && blend_mode <= 3 {
            return blit_8bpp(
                dst,
                dst_x,
                dst_y,
                width,
                height,
                src,
                src_x,
                src_y,
                color_table,
                flags,
            );
        }

        // --- Clipping (shared by 1-bit fast path and generic fallback) ---
        // This mirrors the clipping logic at the top of BitGrid__BlitSpriteRect.
        let dst_right = dst_x + width;
        let dst_bottom = dst_y + height;

        let clip_left = (*dst).clip_left as i32;
        let clip_top = (*dst).clip_top as i32;
        let clip_right = (*dst).clip_right as i32;
        let clip_bottom = (*dst).clip_bottom as i32;

        // Early-out: completely outside clip rect
        if dst_x >= clip_right
            || dst_right <= clip_left
            || dst_y >= clip_bottom
            || dst_bottom <= clip_top
        {
            return 0;
        }

        // Clamp visible region to clip rect
        let vis_left = dst_x.max(clip_left);
        let vis_right = dst_right.min(clip_right);
        let vis_top = dst_y.max(clip_top);
        let vis_bottom = dst_bottom.min(clip_bottom);

        if vis_left >= vis_right || vis_top >= vis_bottom {
            return 0;
        }

        // Adjust source coordinates for clipping and orientation.
        // The orientation switch in the original adjusts src_x/src_y before the cpp check.
        let orientation_code = (flags >> 16) & 0xFFFF;
        let (adj_src_x, adj_src_y) = adjust_src_for_orientation(
            src_x,
            src_y,
            dst_x,
            dst_y,
            width,
            height,
            vis_left,
            vis_top,
            vis_right,
            vis_bottom,
            orientation_code,
        );

        // Build slices from raw BitGrid data pointers
        let dst_data = core::slice::from_raw_parts_mut(
            (*dst).data,
            ((*dst).row_stride * (*dst).height) as usize,
        );
        let src_data =
            core::slice::from_raw_parts((*src).data, ((*src).row_stride * (*src).height) as usize);

        // 1-bit byte-aligned fast path
        if dst_cpp == 1 && src_cpp == 1 {
            let left_aligned = (vis_left & 7) == 0;
            let right_aligned = (vis_right & 7) == 0;
            let src_aligned = (adj_src_x & 7) == 0;

            if left_aligned && right_aligned && src_aligned && color_table.is_null() && flags < 4 {
                openwa_core::sprite::blit_1bit_aligned(
                    dst_data,
                    (*dst).row_stride,
                    src_data,
                    (*src).row_stride,
                    vis_left,
                    vis_top,
                    vis_right,
                    vis_bottom,
                    adj_src_x,
                    adj_src_y,
                    blend_mode,
                );
                return 1;
            }
        }

        // Convert color_table pointer to typed reference for the generic path
        let color_table_ref: Option<&[u8; 256]> = if !color_table.is_null() {
            Some(&*(color_table as *const [u8; 256]))
        } else {
            None
        };

        // Generic per-pixel fallback — handles all remaining cases
        openwa_core::sprite::blit_generic_perpixel(
            dst_data,
            (*dst).row_stride,
            dst_cpp,
            src_data,
            (*src).row_stride,
            src_cpp,
            vis_left,
            vis_top,
            vis_right,
            vis_bottom,
            adj_src_x,
            adj_src_y,
            color_table_ref,
            blend_mode,
        )
    }
}

/// Adjust source coordinates for clipping and orientation.
///
/// Port of the orientation switch in BitGrid__BlitSpriteRect (0x4F69D3..0x4F6A9D).
/// Maps the 16 orientation codes (0-15) to 8 unique source adjustments.
pub fn adjust_src_for_orientation(
    src_x: i32,
    src_y: i32,
    dst_x: i32,
    dst_y: i32,
    width: i32,
    height: i32,
    vis_left: i32,
    vis_top: i32,
    vis_right: i32,
    vis_bottom: i32,
    orientation: u32,
) -> (i32, i32) {
    match orientation {
        1 | 10 => {
            // MirrorX: src_x from right side, src_y from top
            let sx = src_x + (dst_x - vis_right) + width;
            let sy = src_y + (vis_top - dst_y);
            (sx, sy)
        }
        2 | 9 => {
            // MirrorY: src_x from left, src_y from bottom
            let sx = src_x + (vis_left - dst_x);
            let sy = src_y + (dst_y - vis_bottom) + height;
            (sx, sy)
        }
        3 | 8 => {
            // MirrorXY: src_x from right, src_y from bottom
            let sx = src_x + (dst_x - vis_right) + width;
            let sy = src_y + (dst_y - vis_bottom) + height;
            (sx, sy)
        }
        4 | 15 => {
            // Rotate90: src_x tracks dst_y from bottom, src_y tracks dst_x from left
            let sx = src_x + (dst_y - vis_bottom) + height;
            let sy = src_y + (vis_left - dst_x);
            (sx, sy)
        }
        5 | 14 => {
            // Rotate90+MirrorX
            let sx = src_x + (dst_y - vis_bottom) + height;
            let sy = src_y + (dst_x - vis_right) + width;
            (sx, sy)
        }
        6 | 13 => {
            // Rotate90+MirrorY
            let sx = src_x + (vis_top - dst_y);
            let sy = src_y + (vis_left - dst_x);
            (sx, sy)
        }
        7 | 12 => {
            // Rotate90+MirrorXY
            let sx = src_x + (vis_top - dst_y);
            let sy = src_y + (dst_x - vis_right) + width;
            (sx, sy)
        }
        _ => {
            // Normal (0, 11, or any default)
            let sx = src_x + (vis_left - dst_x);
            let sy = src_y + (vis_top - dst_y);
            (sx, sy)
        }
    }
}

/// 8bpp blit path — extracted from the original blit_impl for clarity.
///
/// Handles blend modes 0-3 on 8bpp surfaces via PixelGrid wrapper.
pub unsafe fn blit_8bpp(
    dst: *mut DisplayBitGrid,
    dst_x: i32,
    dst_y: i32,
    width: i32,
    height: i32,
    src: *mut DisplayBitGrid,
    src_x: i32,
    src_y: i32,
    color_table: *const u8,
    flags: u32,
) -> u32 {
    unsafe {
        let color_table_ref: Option<&[u8; 256]> = if !color_table.is_null() {
            Some(&*(color_table as *const [u8; 256]))
        } else {
            None
        };

        let orientation = BlitOrientation::from_flags(flags);
        let blend = BlitBlend::from_flags(flags);

        let src_data =
            core::slice::from_raw_parts((*src).data, ((*src).row_stride * (*src).height) as usize);
        let blit_src = BlitSource {
            data: src_data,
            width: (*src).width,
            height: (*src).height,
            row_stride: (*src).row_stride,
        };

        let dst_data_len = ((*dst).row_stride * (*dst).height) as usize;
        let dst_grid = PixelGridMut {
            data: core::slice::from_raw_parts_mut((*dst).data, dst_data_len),
            width: (*dst).width,
            height: (*dst).height,
            row_stride: (*dst).row_stride,
            clip_left: (*dst).clip_left,
            clip_top: (*dst).clip_top,
            clip_right: (*dst).clip_right,
            clip_bottom: (*dst).clip_bottom,
        };

        blit_sprite_rect(
            dst_grid,
            &blit_src,
            dst_x,
            dst_y,
            width,
            height,
            src_x,
            src_y,
            color_table_ref,
            orientation,
            blend,
        ) as u32
    }
}

/// Stippled blit on raw BitGrid pointers.
///
/// Wraps the raw pointers into PixelGrid/BlitSource and calls
/// `sprite_blit::blit_stippled`.
pub unsafe fn blit_stippled_raw(
    dst: *mut DisplayBitGrid,
    src: *mut DisplayBitGrid,
    dst_x: i32,
    dst_y: i32,
    width: i32,
    height: i32,
    src_x: i32,
    src_y: i32,
    stipple_mode: u32,
    parity: u32,
) {
    unsafe {
        let src_data =
            core::slice::from_raw_parts((*src).data, ((*src).row_stride * (*src).height) as usize);
        let blit_src = BlitSource {
            data: src_data,
            width: (*src).width,
            height: (*src).height,
            row_stride: (*src).row_stride,
        };

        let dst_data_len = ((*dst).row_stride * (*dst).height) as usize;
        let mut dst_grid = PixelGridMut {
            data: core::slice::from_raw_parts_mut((*dst).data, dst_data_len),
            width: (*dst).width,
            height: (*dst).height,
            row_stride: (*dst).row_stride,
            clip_left: (*dst).clip_left,
            clip_top: (*dst).clip_top,
            clip_right: (*dst).clip_right,
            clip_bottom: (*dst).clip_bottom,
        };

        blit_stippled(
            &mut dst_grid,
            &blit_src,
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
}

/// Tiled blit on raw BitGrid pointers.
///
/// Wraps the raw pointers into PixelGrid/BlitSource and calls
/// `sprite_blit::blit_tiled`.
pub unsafe fn blit_tiled_raw(
    dst: *mut DisplayBitGrid,
    src: *mut DisplayBitGrid,
    initial_x: i32,
    dst_y: i32,
    tile_width: i32,
    tile_height: i32,
    clip_left: i32,
    clip_right: i32,
    flags: u32,
) {
    unsafe {
        let src_data =
            core::slice::from_raw_parts((*src).data, ((*src).row_stride * (*src).height) as usize);
        let blit_src = BlitSource {
            data: src_data,
            width: (*src).width,
            height: (*src).height,
            row_stride: (*src).row_stride,
        };

        let dst_data_len = ((*dst).row_stride * (*dst).height) as usize;
        let mut dst_grid = PixelGridMut {
            data: core::slice::from_raw_parts_mut((*dst).data, dst_data_len),
            width: (*dst).width,
            height: (*dst).height,
            row_stride: (*dst).row_stride,
            clip_left: (*dst).clip_left,
            clip_top: (*dst).clip_top,
            clip_right: (*dst).clip_right,
            clip_bottom: (*dst).clip_bottom,
        };

        blit_tiled(
            &mut dst_grid,
            &blit_src,
            initial_x,
            dst_y,
            tile_width,
            tile_height,
            clip_left,
            clip_right,
            None, // color_table — tiled mode doesn't use one
            flags,
        );
    }
}
