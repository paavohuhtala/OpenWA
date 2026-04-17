//! Sprite blitting algorithms for 8bpp BitGrid surfaces.
//!
//! Pure Rust port of WA's core blit pipeline (`FUN_004f6910`).
//!
//! The blit function copies a rectangular region from a source surface to a
//! destination surface with:
//!
//! - Clipping to the destination's clip rectangle
//! - 16 orientation modes (identity, mirror, 90°/180°/270° rotation, and combos)
//! - Blend modes: direct copy, color-table remap, additive, subtractive
//!
//! ## Architecture
//!
//! ```text
//! blit_sprite_rect()
//!   ├── Clip dest rect to dst clip bounds
//!   ├── Adjust src offsets based on orientation
//!   └── Copy pixels (row-major scan)
//!       ├── Mode 0: direct copy (memcpy-like)
//!       ├── Mode 1: color-table blend
//!       ├── Mode 2: additive color mix
//!       └── Mode 3: subtractive color mix
//! ```

use super::line_draw::{PixelGrid, PixelGridMut};

/// Source surface for blit operations.
///
/// Wraps an 8bpp indexed pixel buffer with dimensions and stride.
/// This is the pure-Rust equivalent of a BitGrid used as a blit source.
pub struct BlitSource<'a> {
    pub data: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub row_stride: u32,
}

impl<'a> From<&'a PixelGrid> for BlitSource<'a> {
    fn from(grid: &'a PixelGrid) -> Self {
        BlitSource {
            data: &grid.data,
            width: grid.width,
            height: grid.height,
            row_stride: grid.row_stride,
        }
    }
}

/// Orientation for sprite blitting (high 16 bits of flags).
///
/// WA supports 16 orientations (0-15) which combine rotation and mirroring.
/// The orientation determines how source coordinates map to destination coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum BlitOrientation {
    /// No transform — source maps directly to destination.
    Normal = 0,
    /// Horizontal mirror (flip X).
    MirrorX = 1,
    /// Vertical mirror (flip Y).
    MirrorY = 2,
    /// Mirror X + Y (180° rotation).
    MirrorXY = 3,
    /// 90° clockwise rotation.
    Rotate90 = 4,
    /// 90° CW + mirror X.
    Rotate90MirrorX = 5,
    /// 90° CW + mirror Y.
    Rotate90MirrorY = 6,
    /// 90° CW + mirror XY (= 270° CW).
    Rotate90MirrorXY = 7,
}

impl BlitOrientation {
    /// Parse orientation from the high 16 bits of the flags word.
    /// Values 8-15 mirror values 0-7 (they wrap with paired symmetry in WA).
    pub fn from_flags(flags: u32) -> Self {
        match (flags >> 16) & 0xFFFF {
            0 | 11 => Self::Normal,
            1 | 10 => Self::MirrorX,
            2 | 9 => Self::MirrorY,
            3 | 8 => Self::MirrorXY,
            4 | 15 => Self::Rotate90,
            5 | 14 => Self::Rotate90MirrorX,
            6 | 13 => Self::Rotate90MirrorY,
            7 | 12 => Self::Rotate90MirrorXY,
            _ => Self::Normal,
        }
    }
}

/// Blend mode for sprite blitting (low 16 bits of flags).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlitBlend {
    /// Direct copy — source pixels replace destination.
    /// Transparent pixels (index 0) are skipped.
    Copy,
    /// Color-table blend — each source pixel is remapped through a 256-byte LUT.
    /// If `color_table` is None, uses direct copy with transparency.
    ColorTable,
    /// Additive mask — copy source pixel only where both src and dst are non-zero.
    /// Used for overlay effects (paint only on existing content).
    Additive,
    /// Subtractive mask — copy source pixel only where src is non-zero and dst is zero.
    /// Used for gap-fill effects (paint only where destination is empty).
    Subtractive,
}

impl BlitBlend {
    pub fn from_flags(flags: u32) -> Self {
        match flags & 0xFFFF {
            0 => Self::Copy,
            1 => Self::ColorTable,
            2 => Self::Additive,
            3 => Self::Subtractive,
            _ => Self::Copy,
        }
    }
}

/// Blit a rectangular region from `src` onto `dst`.
///
/// This is the pure Rust port of WA's core blit function (0x4F6910).
///
/// # Parameters
///
/// - `dst`: Destination pixel grid (written to).
/// - `src`: Source surface (read from).
/// - `dst_x`, `dst_y`: Top-left destination position.
/// - `width`, `height`: Size of the region to blit.
/// - `src_x`, `src_y`: Offset into the source surface.
/// - `color_table`: Optional 256-byte LUT for color remapping (blend mode 1).
/// - `orientation`: Transform to apply.
/// - `blend`: Pixel compositing mode.
///
/// Returns `true` if any pixels were drawn.
pub fn blit_sprite_rect(
    mut dst: PixelGridMut<'_>,
    src: &BlitSource,
    dst_x: i32,
    dst_y: i32,
    width: i32,
    height: i32,
    src_x: i32,
    src_y: i32,
    color_table: Option<&[u8; 256]>,
    orientation: BlitOrientation,
    blend: BlitBlend,
) -> bool {
    if width == 0 || height == 0 {
        return false;
    }

    // Destination rect before clipping
    let dst_right = dst_x + width;
    let dst_bottom = dst_y + height;

    // Clip bounds
    let clip_left = dst.clip_left as i32;
    let clip_top = dst.clip_top as i32;
    let clip_right = dst.clip_right as i32;
    let clip_bottom = dst.clip_bottom as i32;

    // Early-out: completely outside clip rect
    if dst_x >= clip_right
        || dst_right <= clip_left
        || dst_y >= clip_bottom
        || dst_bottom <= clip_top
    {
        return false;
    }

    // Clamp to clip rect
    let vis_left = dst_x.max(clip_left);
    let vis_right = dst_right.min(clip_right);
    let vis_top = dst_y.max(clip_top);
    let vis_bottom = dst_bottom.min(clip_bottom);

    if vis_left >= vis_right || vis_top >= vis_bottom {
        return false;
    }

    // Calculate source offsets after clipping, adjusted for orientation.
    // The orientation determines how clipped edges map to source coordinates.
    let vis_w = vis_right - vis_left;
    let vis_h = vis_bottom - vis_top;

    match blend {
        BlitBlend::Copy => {
            // WA's mode 0 (direct copy) is always a forward memcpy.
            // Orientation is ignored — the DisplayGfx layer handles visual
            // mirroring by adjusting destination coordinates before calling
            // the core blit. Source offset uses Normal orientation.
            let (sx_start, sy_start) = adjust_source_for_clip(
                BlitOrientation::Normal,
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
            );
            blit_copy(
                &mut dst, src, vis_left, vis_top, vis_w, vis_h, sx_start, sy_start, 1, 1,
            );
        }
        BlitBlend::ColorTable => {
            // WA's mode 1 uses orientation-specific inner loops:
            // orientation 0 = forward copy, orientation 1 = reverse (mirrored).
            let (sx_start, sy_start) = adjust_source_for_clip(
                orientation,
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
            );
            let (sx_step, sy_step, swap_axes) = orientation_steps(orientation);
            let table = color_table.unwrap_or(&IDENTITY_TABLE);
            if swap_axes {
                blit_color_table_swapped(
                    &mut dst, src, vis_left, vis_top, vis_w, vis_h, sx_start, sy_start, sx_step,
                    sy_step, table,
                );
            } else {
                blit_color_table(
                    &mut dst, src, vis_left, vis_top, vis_w, vis_h, sx_start, sy_start, sx_step,
                    sy_step, table,
                );
            }
        }
        BlitBlend::Additive => {
            // WA's mode 2: forward scan only (orientation ignored in inner loop).
            let (sx_start, sy_start) = adjust_source_for_clip(
                BlitOrientation::Normal,
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
            );
            blit_masked(
                &mut dst,
                src,
                vis_left,
                vis_top,
                vis_w,
                vis_h,
                sx_start,
                sy_start,
                |s, d| {
                    // Write src where both src and dst are non-zero
                    s != 0 && d != 0
                },
            );
        }
        BlitBlend::Subtractive => {
            // WA's mode 3: forward scan only (orientation ignored in inner loop).
            let (sx_start, sy_start) = adjust_source_for_clip(
                BlitOrientation::Normal,
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
            );
            blit_masked(
                &mut dst,
                src,
                vis_left,
                vis_top,
                vis_w,
                vis_h,
                sx_start,
                sy_start,
                |s, d| {
                    // Write src where src is non-zero and dst is zero
                    s != 0 && d == 0
                },
            );
        }
    }

    true
}

/// Identity color table (no remapping).
const IDENTITY_TABLE: [u8; 256] = {
    let mut t = [0u8; 256];
    let mut i = 0;
    while i < 256 {
        t[i] = i as u8;
        i += 1;
    }
    t
};

/// Calculate the starting source coordinates after clipping, accounting for orientation.
///
/// When stepping forward (+1), we skip clipped pixels from the start:
///   start = src_origin + clipped_count
/// When stepping backward (-1), we start from the far end minus clipped pixels:
///   start = src_origin + extent - 1 - clipped_count
fn adjust_source_for_clip(
    orientation: BlitOrientation,
    src_x: i32,
    src_y: i32,
    dst_x: i32,
    dst_y: i32,
    width: i32,
    height: i32,
    vis_left: i32,
    vis_top: i32,
    _vis_right: i32,
    _vis_bottom: i32,
) -> (i32, i32) {
    use BlitOrientation::*;

    let clip_left = vis_left - dst_x; // pixels clipped from left
    let clip_top = vis_top - dst_y; // pixels clipped from top

    // For non-rotated modes: dst_x maps to src_x, dst_y maps to src_y.
    // For rotated modes (swap_axes): dst_y maps to src_x, dst_x maps to src_y.
    match orientation {
        Normal => (src_x + clip_left, src_y + clip_top),
        MirrorX => (src_x + width - 1 - clip_left, src_y + clip_top),
        MirrorY => (src_x + clip_left, src_y + height - 1 - clip_top),
        MirrorXY => (src_x + width - 1 - clip_left, src_y + height - 1 - clip_top),
        // Rotated: src_x tracks dst_y, src_y tracks dst_x
        Rotate90 => (src_x + clip_top, src_y + clip_left),
        Rotate90MirrorX => (src_x + clip_top, src_y + width - 1 - clip_left),
        Rotate90MirrorY => (src_x + height - 1 - clip_top, src_y + clip_left),
        Rotate90MirrorXY => (src_x + height - 1 - clip_top, src_y + width - 1 - clip_left),
    }
}

/// Returns (x_step, y_step, swap_axes) for the given orientation.
///
/// For non-swapped modes: outer loop is dst Y, inner is dst X.
///   source advances by (sx_step, 0) per dst X pixel, (0, sy_step) per dst Y row.
///
/// For swapped modes (90° rotations): source X advances with dst Y, source Y with dst X.
fn orientation_steps(orientation: BlitOrientation) -> (i32, i32, bool) {
    use BlitOrientation::*;
    match orientation {
        Normal => (1, 1, false),
        MirrorX => (-1, 1, false),
        MirrorY => (1, -1, false),
        MirrorXY => (-1, -1, false),
        Rotate90 => (1, 1, true),
        Rotate90MirrorX => (1, -1, true),
        Rotate90MirrorY => (-1, 1, true),
        Rotate90MirrorXY => (-1, -1, true),
    }
}

// ---------------------------------------------------------------------------
// Inner blit loops
// ---------------------------------------------------------------------------

/// Direct copy blit — non-rotated orientations.
///
/// Source is scanned with sx advancing per dst X, sy advancing per dst Y.
fn blit_copy(
    dst: &mut PixelGridMut<'_>,
    src: &BlitSource,
    vis_left: i32,
    vis_top: i32,
    vis_w: i32,
    vis_h: i32,
    sx_start: i32,
    sy_start: i32,
    sx_step: i32,
    sy_step: i32,
) {
    let dst_stride = dst.row_stride as usize;
    let src_stride = src.row_stride as usize;

    let mut sy = sy_start;
    for dy in 0..vis_h {
        let dst_row = (vis_top + dy) as usize * dst_stride + vis_left as usize;
        let src_row = sy as usize * src_stride;

        if sx_step == 1 {
            // Forward copy — can use slice copy
            let sx = sx_start as usize;
            let src_slice = &src.data[src_row + sx..src_row + sx + vis_w as usize];
            dst.data[dst_row..dst_row + vis_w as usize].copy_from_slice(src_slice);
        } else {
            // Reverse copy (mirrored X)
            let mut sx = sx_start;
            for dx in 0..vis_w {
                dst.data[dst_row + dx as usize] = src.data[src_row + sx as usize];
                sx += sx_step;
            }
        }

        sy += sy_step;
    }
}

/// Color-table blend blit — non-rotated orientations.
///
/// Each non-zero source pixel is looked up in the color table before writing.
/// Zero (transparent) pixels are skipped.
fn blit_color_table(
    dst: &mut PixelGridMut<'_>,
    src: &BlitSource,
    vis_left: i32,
    vis_top: i32,
    vis_w: i32,
    vis_h: i32,
    sx_start: i32,
    sy_start: i32,
    sx_step: i32,
    sy_step: i32,
    table: &[u8; 256],
) {
    let dst_stride = dst.row_stride as usize;
    let src_stride = src.row_stride as usize;

    let mut sy = sy_start;
    for dy in 0..vis_h {
        let dst_row = (vis_top + dy) as usize * dst_stride + vis_left as usize;
        let src_row = sy as usize * src_stride;

        let mut sx = sx_start;
        for dx in 0..vis_w {
            let pixel = src.data[src_row + sx as usize];
            if pixel != 0 {
                dst.data[dst_row + dx as usize] = table[pixel as usize];
            }
            sx += sx_step;
        }

        sy += sy_step;
    }
}

/// Color-table blend blit — 90° rotated orientations (axes swapped).
fn blit_color_table_swapped(
    dst: &mut PixelGridMut<'_>,
    src: &BlitSource,
    vis_left: i32,
    vis_top: i32,
    vis_w: i32,
    vis_h: i32,
    sx_start: i32,
    sy_start: i32,
    sx_step: i32,
    sy_step: i32,
    table: &[u8; 256],
) {
    let dst_stride = dst.row_stride as usize;
    let src_stride = src.row_stride as usize;

    let mut src_x = sx_start;
    for dy in 0..vis_h {
        let dst_row = (vis_top + dy) as usize * dst_stride + vis_left as usize;

        let mut src_y = sy_start;
        for dx in 0..vis_w {
            let pixel = src.data[src_x as usize * src_stride + src_y as usize];
            if pixel != 0 {
                dst.data[dst_row + dx as usize] = table[pixel as usize];
            }
            src_y += sy_step;
        }

        src_x += sx_step;
    }
}

/// Masked blit — forward scan only (no orientation).
///
/// Copies source pixels to destination where the predicate `should_write(src, dst)` is true.
/// Used by modes 2 (additive) and 3 (subtractive).
fn blit_masked(
    dst: &mut PixelGridMut<'_>,
    src: &BlitSource,
    vis_left: i32,
    vis_top: i32,
    vis_w: i32,
    vis_h: i32,
    sx_start: i32,
    sy_start: i32,
    should_write: impl Fn(u8, u8) -> bool,
) {
    let dst_stride = dst.row_stride as usize;
    let src_stride = src.row_stride as usize;

    for (sy, dy) in (sy_start..).zip(0..vis_h) {
        let dst_row = (vis_top + dy) as usize * dst_stride + vis_left as usize;
        let src_row = sy as usize * src_stride;

        for (sx, dx) in (sx_start..).zip(0..vis_w) {
            let src_pixel = src.data[src_row + sx as usize];
            let dst_pixel = dst.data[dst_row + dx as usize];
            if should_write(src_pixel, dst_pixel) {
                dst.data[dst_row + dx as usize] = src_pixel;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Test sprite creation from images
// ---------------------------------------------------------------------------

/// Create a PixelGrid from raw 8bpp indexed pixel data.
///
/// This is the simplest way to create test sprites — provide raw palette indices.
pub fn pixel_grid_from_indexed(width: u32, height: u32, pixels: &[u8]) -> PixelGrid {
    assert_eq!(
        pixels.len(),
        (width * height) as usize,
        "pixel data size must match width * height"
    );

    let mut grid = PixelGrid::new(width, height);
    for y in 0..height {
        let src_row = (y * width) as usize;
        let dst_row = (y * grid.row_stride) as usize;
        grid.data[dst_row..dst_row + width as usize]
            .copy_from_slice(&pixels[src_row..src_row + width as usize]);
    }
    grid
}

// ---------------------------------------------------------------------------
// 1-bit blit (collision masks)
// ---------------------------------------------------------------------------

/// Get a single bit from a 1-bit BitGrid data buffer.
///
/// Bit layout matches WA's CollisionBitGrid (0x4F5D70):
/// byte = data[y * stride + (x >> 3)], bit = (byte >> (x & 7)) & 1
#[inline]
fn bit_get(data: &[u8], stride: u32, x: i32, y: i32) -> u8 {
    let byte = data[(y as u32 * stride + (x as u32 >> 3)) as usize];
    (byte >> (x & 7)) & 1
}

/// Set a single bit in a 1-bit BitGrid data buffer.
///
/// Matches WA's CollisionBitGrid__Put (0x4F5DA0):
/// clear the bit, then OR in the new value.
#[inline]
fn bit_put(data: &mut [u8], stride: u32, x: i32, y: i32, value: u8) {
    let idx = (y as u32 * stride + (x as u32 >> 3)) as usize;
    let bit = (x & 7) as u8;
    data[idx] = (data[idx] & !(1 << bit)) | (((value != 0) as u8) << bit);
}

/// Byte-aligned 1-bit blit fast path.
///
/// Port of the optimized 1-bit path in BitGrid__BlitSpriteRect (0x4F6910).
/// Requires: both surfaces cpp==1, clip_left/clip_right/src_x all byte-aligned
/// (divisible by 8), no color table, Normal orientation.
///
/// Blend modes:
/// - 0 (Copy): memcpy bytes
/// - 1 (ColorTable) / 3 (Subtractive): bitwise OR (both simplify to OR for 1-bit)
/// - 2 (Additive): no-op (AND of single bits is identity when both are 1)
pub fn blit_1bit_aligned(
    dst: &mut [u8],
    dst_stride: u32,
    src: &[u8],
    src_stride: u32,
    clip_left: i32,
    clip_top: i32,
    clip_right: i32,
    clip_bottom: i32,
    src_x: i32,
    src_y: i32,
    blend_mode: u32,
) {
    let byte_width = ((clip_right - clip_left) >> 3) as usize;
    let dst_byte_x = (clip_left >> 3) as usize;
    let src_byte_x = (src_x >> 3) as usize;

    match blend_mode {
        0 => {
            // Mode 0 (Copy): memcpy per row
            for row in clip_top..clip_bottom {
                let dst_off = row as usize * dst_stride as usize + dst_byte_x;
                let src_row = src_y + (row - clip_top);
                let src_off = src_row as usize * src_stride as usize + src_byte_x;
                dst[dst_off..dst_off + byte_width]
                    .copy_from_slice(&src[src_off..src_off + byte_width]);
            }
        }
        2 => {
            // Mode 2 (Additive): no-op for 1-bit
        }
        _ => {
            // Mode 1 (ColorTable) and 3 (Subtractive): bitwise OR
            // Both simplify to OR for single-bit values.
            for row in clip_top..clip_bottom {
                let dst_off = row as usize * dst_stride as usize + dst_byte_x;
                let src_row = src_y + (row - clip_top);
                let src_off = src_row as usize * src_stride as usize + src_byte_x;
                for i in 0..byte_width {
                    dst[dst_off + i] |= src[src_off + i];
                }
            }
        }
    }
}

/// Generic per-pixel blit fallback.
///
/// Port of FUN_004f80c0 — handles all blend modes (0-5) for any cells_per_unit
/// by operating one pixel at a time. Slower than the specialized fast paths but
/// handles unaligned 1-bit blits, oriented blits that fell through, and blend
/// modes 4 (erase) and 5 (collision test).
///
/// Parameters match the original's calling convention:
/// - clip_left..clip_right, clip_top..clip_bottom: destination region (already clipped)
/// - src_x: source X corresponding to clip_left
/// - src_y: source Y corresponding to clip_top
/// - blend_mode: 0=copy, 1=color_table, 2=additive, 3=subtractive, 4=erase, 5=collision_test
///
/// Returns 1 normally, or for mode 5: 1 if collision detected, 0 if not.
pub fn blit_generic_perpixel(
    dst: &mut [u8],
    dst_stride: u32,
    dst_cpp: u32,
    src: &[u8],
    src_stride: u32,
    src_cpp: u32,
    clip_left: i32,
    clip_top: i32,
    clip_right: i32,
    clip_bottom: i32,
    src_x: i32,
    src_y: i32,
    color_table: Option<&[u8; 256]>,
    blend_mode: u32,
) -> u32 {
    // Pixel accessors based on cells_per_unit
    #[inline]
    fn get_pixel(data: &[u8], stride: u32, cpp: u32, x: i32, y: i32) -> u8 {
        if cpp == 1 {
            bit_get(data, stride, x, y)
        } else {
            data[y as usize * stride as usize + x as usize]
        }
    }

    #[inline]
    fn put_pixel(data: &mut [u8], stride: u32, cpp: u32, x: i32, y: i32, value: u8) {
        if cpp == 1 {
            bit_put(data, stride, x, y, value);
        } else {
            data[y as usize * stride as usize + x as usize] = value;
        }
    }

    let src_x_offset = src_x - clip_left;

    match blend_mode {
        0 => {
            // Copy: get from src, put to dst
            for y in clip_top..clip_bottom {
                for x in clip_left..clip_right {
                    let pixel = get_pixel(
                        src,
                        src_stride,
                        src_cpp,
                        src_x_offset + x,
                        src_y + (y - clip_top),
                    );
                    put_pixel(dst, dst_stride, dst_cpp, x, y, pixel);
                }
            }
            1
        }
        1 => {
            // Color table blend — per-pixel with optional LUT
            for y in clip_top..clip_bottom {
                for x in clip_left..clip_right {
                    let pixel = get_pixel(
                        src,
                        src_stride,
                        src_cpp,
                        src_x_offset + x,
                        src_y + (y - clip_top),
                    );
                    if pixel != 0 {
                        let value = match color_table {
                            Some(table) => table[pixel as usize],
                            None => pixel,
                        };
                        put_pixel(dst, dst_stride, dst_cpp, x, y, value);
                    }
                }
            }
            1
        }
        2 => {
            // Additive: copy src where both src and dst non-zero
            for y in clip_top..clip_bottom {
                for x in clip_left..clip_right {
                    let src_pixel = get_pixel(
                        src,
                        src_stride,
                        src_cpp,
                        src_x_offset + x,
                        src_y + (y - clip_top),
                    );
                    if src_pixel != 0 {
                        let dst_pixel = get_pixel(dst, dst_stride, dst_cpp, x, y);
                        if dst_pixel != 0 {
                            put_pixel(dst, dst_stride, dst_cpp, x, y, src_pixel);
                        }
                    }
                }
            }
            1
        }
        3 => {
            // Subtractive: copy src where src non-zero and dst zero
            for y in clip_top..clip_bottom {
                for x in clip_left..clip_right {
                    let src_pixel = get_pixel(
                        src,
                        src_stride,
                        src_cpp,
                        src_x_offset + x,
                        src_y + (y - clip_top),
                    );
                    if src_pixel != 0 {
                        let dst_pixel = get_pixel(dst, dst_stride, dst_cpp, x, y);
                        if dst_pixel == 0 {
                            put_pixel(dst, dst_stride, dst_cpp, x, y, src_pixel);
                        }
                    }
                }
            }
            1
        }
        4 => {
            // Erase: clear dst where src non-zero
            for y in clip_top..clip_bottom {
                for x in clip_left..clip_right {
                    let src_pixel = get_pixel(
                        src,
                        src_stride,
                        src_cpp,
                        src_x_offset + x,
                        src_y + (y - clip_top),
                    );
                    if src_pixel != 0 {
                        put_pixel(dst, dst_stride, dst_cpp, x, y, 0);
                    }
                }
            }
            1
        }
        5 => {
            // Collision test: return 1 if any pixel overlaps (both non-zero)
            for y in clip_top..clip_bottom {
                for x in clip_left..clip_right {
                    let dst_pixel = get_pixel(dst, dst_stride, dst_cpp, x, y);
                    if dst_pixel != 0 {
                        let src_pixel = get_pixel(
                            src,
                            src_stride,
                            src_cpp,
                            src_x_offset + x,
                            src_y + (y - clip_top),
                        );
                        if src_pixel != 0 {
                            return 1;
                        }
                    }
                }
            }
            0
        }
        _ => 1,
    }
}

// ---------------------------------------------------------------------------
// Stippled blit (checkerboard pattern)
// ---------------------------------------------------------------------------

/// Blit a sprite with a checkerboard (stippled) pattern.
///
/// Port of DisplayGfx__BlitStippled (0x56AEF0). Draws every other pixel
/// in a checkerboard pattern, creating a dithered transparency effect.
///
/// The checkerboard is determined by: `(dst_x ^ parity ^ dst_y ^ mode) & 1`.
/// - `parity` alternates 0/1 each frame (g_StippleParity at 0x7A087C)
/// - `mode` is 0 or 1, inverting the pattern between the two stippled flag bits
///
/// Only non-zero source pixels are drawn (transparent = 0).
/// Pixels outside the destination clip rect are skipped.
pub fn blit_stippled(
    dst: &mut PixelGridMut<'_>,
    src: &BlitSource<'_>,
    dst_x: i32,
    dst_y: i32,
    width: i32,
    height: i32,
    src_x: i32,
    src_y: i32,
    stipple_mode: u32,
    parity: u32,
) {
    if width <= 0 || height <= 0 {
        return;
    }

    // Source-to-destination offset (for mapping dst coords back to src)
    let src_offset_x = src_x - dst_x;

    for row in 0..height {
        let sy = src_y + row;
        let dy = (dst_y - src_y) + sy; // = dst_y + row

        // Clip: skip rows outside destination
        if dy < dst.clip_top as i32 || dy >= dst.clip_bottom as i32 {
            continue;
        }

        for col in 0..width {
            let dx = dst_x + col;

            // Clip: skip columns outside destination
            if dx < dst.clip_left as i32 || dx >= dst.clip_right as i32 {
                continue;
            }

            // Checkerboard test — matching WA's XOR pattern
            if (dx as u32 ^ parity ^ dy as u32 ^ stipple_mode) & 1 == 0 {
                continue;
            }

            // Read source pixel (with bounds check matching get_pixel_clipped)
            let sx = src_offset_x + dx;
            if sx < 0 || sx >= src.width as i32 || sy < 0 || sy >= src.height as i32 {
                continue;
            }

            let pixel = src.data[sy as usize * src.row_stride as usize + sx as usize];
            if pixel != 0 {
                dst.data[dy as usize * dst.row_stride as usize + dx as usize] = pixel;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tiled blit (horizontal tiling)
// ---------------------------------------------------------------------------

/// Blit a sprite tiled horizontally across a destination region.
///
/// Port of DisplayGfx__BlitTiled (0x56B000). Tiles the sprite from
/// `clip_left` to `clip_right` by repeatedly calling `blit_sprite_rect`.
///
/// The `initial_x` is wrapped to the largest value <= `clip_left`
/// using the tiling width, then blits are emitted rightward.
///
/// `flags` is the blit flags word (orientation + blend mode) passed through
/// to each individual `blit_sprite_rect` call.
pub fn blit_tiled(
    dst: &mut PixelGridMut<'_>,
    src: &BlitSource<'_>,
    initial_x: i32,
    dst_y: i32,
    tile_width: i32,
    tile_height: i32,
    clip_left: i32,
    clip_right: i32,
    color_table: Option<&[u8; 256]>,
    flags: u32,
) {
    if tile_width <= 0 || tile_height <= 0 {
        return;
    }

    let orientation = BlitOrientation::from_flags(flags);
    let blend = BlitBlend::from_flags(flags);

    // Wrap x to largest value <= clip_left (matching WA's wrapping loop)
    let mut x = initial_x;
    while x < clip_left {
        x += tile_width;
    }
    while x > clip_left {
        x -= tile_width;
    }

    // Tile rightward until past clip_right
    while x < clip_right {
        blit_sprite_rect(
            dst.reborrow(),
            src,
            x,
            dst_y,
            tile_width,
            tile_height,
            0,
            0,
            color_table,
            orientation,
            blend,
        );
        x += tile_width;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a simple 8x8 test pattern with distinct values in each quadrant.
    fn make_test_sprite() -> PixelGrid {
        let mut pixels = vec![0u8; 8 * 8];
        for y in 0..8u32 {
            for x in 0..8u32 {
                // Quadrant coloring: TL=1, TR=2, BL=3, BR=4
                let color = match (x >= 4, y >= 4) {
                    (false, false) => 1,
                    (true, false) => 2,
                    (false, true) => 3,
                    (true, true) => 4,
                };
                pixels[(y * 8 + x) as usize] = color;
            }
        }
        pixel_grid_from_indexed(8, 8, &pixels)
    }

    #[test]
    fn blit_copy_identity() {
        let sprite = make_test_sprite();
        let src = BlitSource::from(&sprite);
        let mut dst = PixelGrid::new(16, 16);

        blit_sprite_rect(
            dst.as_grid_mut(),
            &src,
            4,
            4, // dst position
            8,
            8, // size
            0,
            0, // src offset
            None,
            BlitOrientation::Normal,
            BlitBlend::Copy,
        );

        // Check quadrant corners
        let p = |x: i32, y: i32| dst.data[(y as u32 * dst.row_stride + x as u32) as usize];
        assert_eq!(p(4, 4), 1, "top-left of sprite");
        assert_eq!(p(8, 4), 2, "top-right of sprite");
        assert_eq!(p(4, 8), 3, "bottom-left of sprite");
        assert_eq!(p(8, 8), 4, "bottom-right of sprite");
        // Outside sprite region should be 0
        assert_eq!(p(3, 4), 0, "left of sprite");
        assert_eq!(p(12, 4), 0, "right of sprite");
    }

    #[test]
    fn blit_copy_ignores_orientation() {
        // WA's mode 0 (Copy) is always forward memcpy — orientation has no effect.
        let sprite = make_test_sprite();
        let src = BlitSource::from(&sprite);
        let mut dst_normal = PixelGrid::new(16, 16);
        let mut dst_mirror = PixelGrid::new(16, 16);

        blit_sprite_rect(
            dst_normal.as_grid_mut(),
            &src,
            4,
            4,
            8,
            8,
            0,
            0,
            None,
            BlitOrientation::Normal,
            BlitBlend::Copy,
        );
        blit_sprite_rect(
            dst_mirror.as_grid_mut(),
            &src,
            4,
            4,
            8,
            8,
            0,
            0,
            None,
            BlitOrientation::MirrorX,
            BlitBlend::Copy,
        );

        assert_eq!(
            dst_normal.data, dst_mirror.data,
            "Copy mode ignores orientation"
        );
    }

    #[test]
    fn blit_colortable_mirror_x() {
        // Mirroring works in ColorTable mode (mode 1).
        let sprite = make_test_sprite();
        let src = BlitSource::from(&sprite);
        let mut dst = PixelGrid::new(16, 16);

        blit_sprite_rect(
            dst.as_grid_mut(),
            &src,
            4,
            4,
            8,
            8,
            0,
            0,
            None,
            BlitOrientation::MirrorX,
            BlitBlend::ColorTable,
        );

        let p = |x: i32, y: i32| dst.data[(y as u32 * dst.row_stride + x as u32) as usize];
        assert_eq!(p(4, 4), 2, "mirror: was TR, now TL");
        assert_eq!(p(8, 4), 1, "mirror: was TL, now TR");
        assert_eq!(p(4, 8), 4, "mirror: was BR, now BL");
        assert_eq!(p(8, 8), 3, "mirror: was BL, now BR");
    }

    #[test]
    fn blit_colortable_mirror_y() {
        let sprite = make_test_sprite();
        let src = BlitSource::from(&sprite);
        let mut dst = PixelGrid::new(16, 16);

        blit_sprite_rect(
            dst.as_grid_mut(),
            &src,
            4,
            4,
            8,
            8,
            0,
            0,
            None,
            BlitOrientation::MirrorY,
            BlitBlend::ColorTable,
        );

        let p = |x: i32, y: i32| dst.data[(y as u32 * dst.row_stride + x as u32) as usize];
        assert_eq!(p(4, 4), 3, "mirror-y: was BL, now TL");
        assert_eq!(p(8, 4), 4, "mirror-y: was BR, now TR");
        assert_eq!(p(4, 8), 1, "mirror-y: was TL, now BL");
        assert_eq!(p(8, 8), 2, "mirror-y: was TR, now BR");
    }

    #[test]
    fn blit_copy_clipped() {
        let sprite = make_test_sprite();
        let src = BlitSource::from(&sprite);
        let mut dst = PixelGrid::new(16, 16);
        // Clip to only show bottom-right quadrant of the sprite
        dst.clip_left = 8;
        dst.clip_top = 8;

        blit_sprite_rect(
            dst.as_grid_mut(),
            &src,
            4,
            4,
            8,
            8,
            0,
            0,
            None,
            BlitOrientation::Normal,
            BlitBlend::Copy,
        );

        let p = |x: i32, y: i32| dst.data[(y as u32 * dst.row_stride + x as u32) as usize];
        // Only the bottom-right quadrant should be visible
        assert_eq!(p(7, 7), 0, "clipped away");
        assert_eq!(p(8, 8), 4, "visible bottom-right");
        assert_eq!(p(11, 11), 4, "still bottom-right");
    }

    #[test]
    fn blit_color_table_transparency() {
        let sprite = make_test_sprite();
        let src = BlitSource::from(&sprite);
        let mut dst = PixelGrid::new(16, 16);
        // Fill dst with color 10
        dst.data.fill(10);

        // Remap: 1->11, 2->12, 3->13, 4->14
        let mut table = [0u8; 256];
        for (i, value) in table.iter_mut().enumerate() {
            *value = (i as u8).wrapping_add(10);
        }
        table[0] = 0; // transparent stays 0

        blit_sprite_rect(
            dst.as_grid_mut(),
            &src,
            4,
            4,
            8,
            8,
            0,
            0,
            Some(&table),
            BlitOrientation::Normal,
            BlitBlend::ColorTable,
        );

        let p = |x: i32, y: i32| dst.data[(y as u32 * dst.row_stride + x as u32) as usize];
        assert_eq!(p(4, 4), 11, "remapped color 1 -> 11");
        assert_eq!(p(8, 4), 12, "remapped color 2 -> 12");
        // Outside sprite: background preserved
        assert_eq!(p(3, 4), 10, "background unchanged");
    }

    #[test]
    fn blit_color_table_preserves_transparent() {
        // Create a sprite with some transparent (0) pixels
        let mut pixels = vec![0u8; 8 * 8];
        // Only set pixels on the diagonal
        for i in 0..8 {
            pixels[i * 8 + i] = 5;
        }
        let sprite = pixel_grid_from_indexed(8, 8, &pixels);
        let src = BlitSource::from(&sprite);

        let mut dst = PixelGrid::new(16, 16);
        dst.data.fill(99); // background

        blit_sprite_rect(
            dst.as_grid_mut(),
            &src,
            0,
            0,
            8,
            8,
            0,
            0,
            None,
            BlitOrientation::Normal,
            BlitBlend::ColorTable,
        );

        let p = |x: i32, y: i32| dst.data[(y as u32 * dst.row_stride + x as u32) as usize];
        assert_eq!(p(0, 0), 5, "diagonal pixel drawn");
        assert_eq!(p(1, 0), 99, "transparent pixel preserved background");
        assert_eq!(p(3, 3), 5, "diagonal pixel drawn");
        assert_eq!(p(4, 3), 99, "transparent pixel preserved background");
    }

    #[test]
    fn blit_color_table_lut_remaps_pixels() {
        // Verify that a non-identity LUT actually remaps pixel values.
        let sprite = make_test_sprite(); // quadrants: TL=1, TR=2, BL=3, BR=4
        let src = BlitSource::from(&sprite);
        let mut dst = PixelGrid::new(16, 16);
        dst.data.fill(99); // background

        // LUT: remap 1->10, 2->20, 3->30, 4->40, 0->0 (transparent)
        let mut lut = [0u8; 256];
        lut[1] = 10;
        lut[2] = 20;
        lut[3] = 30;
        lut[4] = 40;

        blit_sprite_rect(
            dst.as_grid_mut(),
            &src,
            4,
            4,
            8,
            8,
            0,
            0,
            Some(&lut),
            BlitOrientation::Normal,
            BlitBlend::ColorTable,
        );

        let p = |x: i32, y: i32| dst.data[(y as u32 * dst.row_stride + x as u32) as usize];
        assert_eq!(p(4, 4), 10, "TL remapped 1->10");
        assert_eq!(p(8, 4), 20, "TR remapped 2->20");
        assert_eq!(p(4, 8), 30, "BL remapped 3->30");
        assert_eq!(p(8, 8), 40, "BR remapped 4->40");
        // Background outside sprite preserved
        assert_eq!(p(3, 4), 99, "background unchanged");
    }

    #[test]
    fn blit_color_table_lut_with_mirror() {
        let sprite = make_test_sprite();
        let src = BlitSource::from(&sprite);
        let mut dst = PixelGrid::new(16, 16);

        let mut lut = [0u8; 256];
        lut[1] = 10;
        lut[2] = 20;
        lut[3] = 30;
        lut[4] = 40;

        blit_sprite_rect(
            dst.as_grid_mut(),
            &src,
            4,
            4,
            8,
            8,
            0,
            0,
            Some(&lut),
            BlitOrientation::MirrorX,
            BlitBlend::ColorTable,
        );

        let p = |x: i32, y: i32| dst.data[(y as u32 * dst.row_stride + x as u32) as usize];
        // MirrorX flips left/right: TL<->TR, BL<->BR
        assert_eq!(p(4, 4), 20, "mirror: TR->TL remapped 2->20");
        assert_eq!(p(8, 4), 10, "mirror: TL->TR remapped 1->10");
        assert_eq!(p(4, 8), 40, "mirror: BR->BL remapped 4->40");
        assert_eq!(p(8, 8), 30, "mirror: BL->BR remapped 3->30");
    }

    #[test]
    fn blit_additive_writes_only_where_both_nonzero() {
        let sprite = make_test_sprite(); // all pixels 1-4 (non-zero)
        let src = BlitSource::from(&sprite);
        let mut dst = PixelGrid::new(16, 16);
        // Fill only the right half of the destination with non-zero
        for y in 0..16u32 {
            for x in 8..16u32 {
                dst.data[(y * dst.row_stride + x) as usize] = 99;
            }
        }

        blit_sprite_rect(
            dst.as_grid_mut(),
            &src,
            4,
            4,
            8,
            8,
            0,
            0,
            None,
            BlitOrientation::Normal,
            BlitBlend::Additive,
        );

        let p = |x: i32, y: i32| dst.data[(y as u32 * dst.row_stride + x as u32) as usize];
        // Left half of sprite (dst_x 4-7): dst was 0, so additive skips
        assert_eq!(p(4, 4), 0, "dst was 0, additive should not write");
        assert_eq!(p(7, 7), 0, "dst was 0, additive should not write");
        // Right half of sprite (dst_x 8-11): dst was 99, so additive writes src
        assert_eq!(p(8, 4), 2, "dst was non-zero, additive writes src (TR)");
        assert_eq!(p(8, 8), 4, "dst was non-zero, additive writes src (BR)");
        // Outside sprite: unchanged
        assert_eq!(p(12, 4), 99, "outside sprite, right half preserved");
        assert_eq!(p(3, 4), 0, "outside sprite, left half preserved");
    }

    #[test]
    fn blit_subtractive_writes_only_where_dst_zero() {
        let sprite = make_test_sprite(); // all pixels 1-4 (non-zero)
        let src = BlitSource::from(&sprite);
        let mut dst = PixelGrid::new(16, 16);
        // Fill only the right half with non-zero
        for y in 0..16u32 {
            for x in 8..16u32 {
                dst.data[(y * dst.row_stride + x) as usize] = 99;
            }
        }

        blit_sprite_rect(
            dst.as_grid_mut(),
            &src,
            4,
            4,
            8,
            8,
            0,
            0,
            None,
            BlitOrientation::Normal,
            BlitBlend::Subtractive,
        );

        let p = |x: i32, y: i32| dst.data[(y as u32 * dst.row_stride + x as u32) as usize];
        // Left half of sprite (dst_x 4-7): dst was 0, so subtractive writes
        assert_eq!(p(4, 4), 1, "dst was 0, subtractive writes src pixel");
        assert_eq!(
            p(7, 7),
            1,
            "dst was 0, subtractive writes src (TL quadrant)"
        );
        // Right half of sprite (dst_x 8-11): dst was 99, so subtractive skips
        assert_eq!(
            p(8, 4),
            99,
            "dst was non-zero, subtractive should not write"
        );
        assert_eq!(
            p(8, 8),
            99,
            "dst was non-zero, subtractive should not write"
        );
    }

    #[test]
    fn blit_additive_skips_transparent_src() {
        // Sprite with transparent (0) pixels should never write them
        let mut pixels = vec![0u8; 8 * 8];
        for i in 0..8 {
            pixels[i * 8 + i] = 5; // diagonal only
        }
        let sprite = pixel_grid_from_indexed(8, 8, &pixels);
        let src = BlitSource::from(&sprite);
        let mut dst = PixelGrid::new(8, 8);
        dst.data.fill(99); // all non-zero

        blit_sprite_rect(
            dst.as_grid_mut(),
            &src,
            0,
            0,
            8,
            8,
            0,
            0,
            None,
            BlitOrientation::Normal,
            BlitBlend::Additive,
        );

        let p = |x: i32, y: i32| dst.data[(y as u32 * dst.row_stride + x as u32) as usize];
        // Diagonal: src=5, dst=99 → both non-zero → writes 5
        assert_eq!(p(0, 0), 5, "diagonal written");
        assert_eq!(p(3, 3), 5, "diagonal written");
        // Off-diagonal: src=0 → skipped even though dst is non-zero
        assert_eq!(p(1, 0), 99, "transparent src not written");
        assert_eq!(p(0, 1), 99, "transparent src not written");
    }

    // =======================================================================
    // Image loading helpers (for snapshot tests with real images)
    // =======================================================================

    /// Load a GIF file as a PixelGrid, preserving raw palette indices.
    ///
    /// GIF is naturally a paletted format (up to 256 colors), so the pixel
    /// values in the returned grid are the original palette indices — exactly
    /// what the blit functions operate on.
    ///
    /// Returns `(grid, palette)` where palette is the RGB color table.
    fn load_gif_indexed(path: &str) -> (PixelGrid, Vec<[u8; 3]>) {
        use gif::DecodeOptions;
        use std::fs::File;

        let file = File::open(path).unwrap_or_else(|e| panic!("Failed to open {path}: {e}"));
        let mut opts = DecodeOptions::new();
        opts.set_color_output(gif::ColorOutput::Indexed);
        let mut decoder = opts
            .read_info(file)
            .unwrap_or_else(|e| panic!("Failed to decode GIF {path}: {e}"));

        // Grab global palette before borrowing frame
        let global_pal = decoder.global_palette().map(|p| p.to_vec());

        let frame = decoder
            .read_next_frame()
            .unwrap_or_else(|e| panic!("Failed to read GIF frame: {e}"))
            .expect("GIF has no frames");

        let w = frame.width as u32;
        let h = frame.height as u32;

        // Extract palette (frame-local or global)
        let palette_bytes = frame
            .palette
            .as_deref()
            .or(global_pal.as_deref())
            .expect("GIF has no palette");
        let palette: Vec<[u8; 3]> = palette_bytes
            .chunks_exact(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect();

        // Remap transparent index to 0 (WA convention).
        // GIF's transparent index can be any palette entry; WA always uses 0.
        let transparent_idx = frame.transparent;
        let mut pixels: Vec<u8> = frame.buffer.to_vec();
        let mut palette = palette;

        if let Some(ti) = transparent_idx {
            if ti != 0 {
                // Swap palette entries 0 and ti, remap all pixels
                palette.swap(0, ti as usize);
                for p in &mut pixels {
                    if *p == 0 {
                        *p = ti;
                    } else if *p == ti {
                        *p = 0;
                    }
                }
            }
        }

        let grid = pixel_grid_from_indexed(w, h, &pixels);
        (grid, palette)
    }

    fn test_asset_path(name: &str) -> String {
        format!(
            "{}/../../testdata/assets/{name}",
            env!("CARGO_MANIFEST_DIR"),
        )
    }

    #[test]
    fn load_sprite_test_gif() {
        let path = test_asset_path("sprite_test.gif");
        let (grid, palette) = load_gif_indexed(&path);
        assert!(grid.width > 0 && grid.height > 0, "image has dimensions");
        assert!(palette.len() <= 256, "palette fits in 8bpp");
        // Verify some pixels are non-zero (not all transparent)
        let nonzero = grid.data.iter().filter(|&&p| p != 0).count();
        assert!(nonzero > 0, "image has non-transparent pixels");
    }

    #[test]
    fn load_transparent_gif() {
        let path = test_asset_path("sprite_transparent_test.gif");
        let (grid, _palette) = load_gif_indexed(&path);
        assert!(grid.width > 0 && grid.height > 0);
        // Should have both transparent (0) and opaque pixels
        let zeros = grid.data.iter().filter(|&&p| p == 0).count();
        let nonzeros = grid.data.iter().filter(|&&p| p != 0).count();
        assert!(zeros > 0, "should have transparent pixels");
        assert!(nonzeros > 0, "should have opaque pixels");
    }

    // =======================================================================
    // Real-image blit tests
    // =======================================================================

    /// Blit the opaque sprite_test.gif at various positions and orientations.
    #[test]
    fn blit_opaque_sprite_identity() {
        let path = test_asset_path("sprite_test.gif");
        let (sprite, _) = load_gif_indexed(&path);
        let src = BlitSource::from(&sprite);
        let sw = sprite.width as i32;
        let sh = sprite.height as i32;

        // Blit centered onto a larger canvas
        let canvas_w = (sw + 32) as u32;
        let canvas_h = (sh + 32) as u32;
        let mut dst = PixelGrid::new(canvas_w, canvas_h);

        blit_sprite_rect(
            dst.as_grid_mut(),
            &src,
            16,
            16,
            sw,
            sh,
            0,
            0,
            None,
            BlitOrientation::Normal,
            BlitBlend::Copy,
        );

        // Border should be untouched (0)
        let p = |x: i32, y: i32| dst.data[(y as u32 * dst.row_stride + x as u32) as usize];
        assert_eq!(p(0, 0), 0, "top-left border");
        assert_eq!(p(15, 16), 0, "left border");

        // Interior should match source
        for y in 0..sh.min(4) {
            for x in 0..sw.min(4) {
                let expected = sprite.data[(y as u32 * sprite.row_stride + x as u32) as usize];
                assert_eq!(p(x + 16, y + 16), expected, "pixel ({x},{y})");
            }
        }
    }

    #[test]
    fn blit_opaque_sprite_mirror_x() {
        // Mirror works in ColorTable mode, not Copy mode.
        let path = test_asset_path("sprite_test.gif");
        let (sprite, _) = load_gif_indexed(&path);
        let src = BlitSource::from(&sprite);
        let sw = sprite.width as i32;
        let sh = sprite.height as i32;

        let mut dst = PixelGrid::new(sprite.width, sprite.height);
        blit_sprite_rect(
            dst.as_grid_mut(),
            &src,
            0,
            0,
            sw,
            sh,
            0,
            0,
            None,
            BlitOrientation::MirrorX,
            BlitBlend::ColorTable,
        );

        // First row: dst[0] should be src[sw-1], dst[1] should be src[sw-2], etc.
        for x in 0..sw {
            let dst_val = dst.data[x as usize];
            let src_val = sprite.data[(sw - 1 - x) as usize];
            assert_eq!(dst_val, src_val, "mirror-x row 0, col {x}");
        }
    }

    #[test]
    fn blit_opaque_sprite_clipped() {
        let path = test_asset_path("sprite_test.gif");
        let (sprite, _) = load_gif_indexed(&path);
        let src = BlitSource::from(&sprite);
        let sw = sprite.width as i32;
        let sh = sprite.height as i32;

        // Blit at (-16, -16) so only bottom-right portion is visible
        let mut dst = PixelGrid::new(sprite.width, sprite.height);
        blit_sprite_rect(
            dst.as_grid_mut(),
            &src,
            -16,
            -16,
            sw,
            sh,
            0,
            0,
            None,
            BlitOrientation::Normal,
            BlitBlend::Copy,
        );

        // dst[0,0] should correspond to src[16,16]
        let dst_val = dst.data[0];
        let src_val = sprite.data[(16 * sprite.row_stride + 16) as usize];
        assert_eq!(dst_val, src_val, "clipped blit: dst(0,0) == src(16,16)");
    }

    /// Blit transparent sprite over a filled background — transparent pixels
    /// should preserve the background.
    #[test]
    fn blit_transparent_sprite_preserves_background() {
        let path = test_asset_path("sprite_transparent_test.gif");
        let (sprite, _) = load_gif_indexed(&path);
        let src = BlitSource::from(&sprite);
        let sw = sprite.width as i32;
        let sh = sprite.height as i32;

        let mut dst = PixelGrid::new(sprite.width, sprite.height);
        dst.data.fill(42); // background color

        blit_sprite_rect(
            dst.as_grid_mut(),
            &src,
            0,
            0,
            sw,
            sh,
            0,
            0,
            None,
            BlitOrientation::Normal,
            BlitBlend::ColorTable,
        );

        // Check that transparent source pixels left the background intact,
        // and opaque pixels overwrote it.
        let mut bg_preserved = 0u32;
        let mut overwritten = 0u32;
        for y in 0..sh {
            for x in 0..sw {
                let src_idx = (y as u32 * sprite.row_stride + x as u32) as usize;
                let dst_idx = (y as u32 * dst.row_stride + x as u32) as usize;
                if sprite.data[src_idx] == 0 {
                    assert_eq!(
                        dst.data[dst_idx], 42,
                        "transparent pixel at ({x},{y}) should preserve background"
                    );
                    bg_preserved += 1;
                } else {
                    assert_ne!(
                        dst.data[dst_idx], 42,
                        "opaque pixel at ({x},{y}) should overwrite background"
                    );
                    overwritten += 1;
                }
            }
        }
        assert!(bg_preserved > 0, "some pixels should be transparent");
        assert!(overwritten > 0, "some pixels should be opaque");
    }

    /// Blit transparent sprite with mirror — verify transparency still works.
    #[test]
    fn blit_transparent_sprite_mirrored() {
        let path = test_asset_path("sprite_transparent_test.gif");
        let (sprite, _) = load_gif_indexed(&path);
        let src = BlitSource::from(&sprite);
        let sw = sprite.width as i32;
        let sh = sprite.height as i32;

        let mut dst = PixelGrid::new(sprite.width, sprite.height);
        dst.data.fill(42);

        blit_sprite_rect(
            dst.as_grid_mut(),
            &src,
            0,
            0,
            sw,
            sh,
            0,
            0,
            None,
            BlitOrientation::MirrorX,
            BlitBlend::ColorTable,
        );

        // Mirrored: dst column x reads from src column (sw-1-x)
        for y in 0..sh.min(8) {
            for x in 0..sw {
                let src_x = sw - 1 - x;
                let src_idx = (y as u32 * sprite.row_stride + src_x as u32) as usize;
                let dst_idx = (y as u32 * dst.row_stride + x as u32) as usize;
                if sprite.data[src_idx] == 0 {
                    assert_eq!(
                        dst.data[dst_idx], 42,
                        "mirrored transparent at dst({x},{y}) from src({src_x},{y})"
                    );
                } else {
                    assert_eq!(
                        dst.data[dst_idx], sprite.data[src_idx],
                        "mirrored opaque at dst({x},{y}) from src({src_x},{y})"
                    );
                }
            }
        }
    }

    // =======================================================================
    // Snapshot tests — compare Rust blit output against WA's native output
    // =======================================================================

    fn load_snapshot(name: &str) -> PixelGrid {
        let path = format!(
            "{}/../../testdata/snapshots/{}.bin",
            env!("CARGO_MANIFEST_DIR"),
            name
        );
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("Failed to load snapshot {name}: {e} (path: {path})"));
        PixelGrid::from_snapshot(&bytes)
    }

    fn assert_matches_snapshot(actual: &PixelGrid, name: &str) {
        let expected = load_snapshot(name);
        assert_eq!(actual.width, expected.width, "{name}: width mismatch");
        assert_eq!(actual.height, expected.height, "{name}: height mismatch");
        assert_eq!(
            actual.row_stride, expected.row_stride,
            "{name}: stride mismatch"
        );
        if actual.data != expected.data {
            let mut diff_count = 0;
            let mut first_diff = None;
            for y in 0..actual.height {
                for x in 0..actual.width {
                    let idx = (y * actual.row_stride + x) as usize;
                    if actual.data[idx] != expected.data[idx] {
                        diff_count += 1;
                        if first_diff.is_none() {
                            first_diff = Some((x, y, actual.data[idx], expected.data[idx]));
                        }
                    }
                }
            }
            let (fx, fy, got, want) = first_diff.unwrap();
            panic!(
                "{name}: {diff_count} pixel(s) differ. First at ({fx},{fy}): got {got}, want {want}"
            );
        }
    }

    /// Run a blit with the same parameters as the DLL capture and compare output.
    ///
    /// `blit_size`: (w, h) override, or None to use sprite dimensions.
    /// `src_offset`: (x, y) source offset.
    fn run_snapshot_test(
        gif_name: &str,
        snap_name: &str,
        dst_x: i32,
        dst_y: i32,
        blit_size: Option<(i32, i32)>,
        src_offset: (i32, i32),
        orientation: BlitOrientation,
        blend: BlitBlend,
        bg_fill: u8,
    ) {
        let path = test_asset_path(gif_name);
        let (sprite, _) = load_gif_indexed(&path);
        let src = BlitSource::from(&sprite);

        let sw = sprite.width as i32;
        let sh = sprite.height as i32;
        let (blit_w, blit_h) = blit_size.unwrap_or((sw, sh));

        let canvas_w = (sw + 32) as u32;
        let canvas_h = (sh + 32) as u32;
        let mut dst = PixelGrid::new(canvas_w, canvas_h);
        dst.data.fill(bg_fill);

        blit_sprite_rect(
            dst.as_grid_mut(),
            &src,
            dst_x,
            dst_y,
            blit_w,
            blit_h,
            src_offset.0,
            src_offset.1,
            None,
            orientation,
            blend,
        );

        assert_matches_snapshot(&dst, snap_name);
    }

    // Snapshot tests — use sprite's own dimensions (matching DLL capture).

    #[test]
    fn snap_blit_opaque_identity() {
        run_snapshot_test(
            "sprite_test.gif",
            "blit_opaque_identity",
            16,
            16,
            None,
            (0, 0),
            BlitOrientation::Normal,
            BlitBlend::Copy,
            0,
        );
    }
    #[test]
    fn snap_blit_opaque_mirror_x() {
        run_snapshot_test(
            "sprite_test.gif",
            "blit_opaque_mirror_x",
            16,
            16,
            None,
            (0, 0),
            BlitOrientation::MirrorX,
            BlitBlend::Copy,
            0,
        );
    }
    #[test]
    fn snap_blit_opaque_mirror_y() {
        run_snapshot_test(
            "sprite_test.gif",
            "blit_opaque_mirror_y",
            16,
            16,
            None,
            (0, 0),
            BlitOrientation::MirrorY,
            BlitBlend::Copy,
            0,
        );
    }
    #[test]
    fn snap_blit_opaque_mirror_xy() {
        run_snapshot_test(
            "sprite_test.gif",
            "blit_opaque_mirror_xy",
            16,
            16,
            None,
            (0, 0),
            BlitOrientation::MirrorXY,
            BlitBlend::Copy,
            0,
        );
    }
    #[test]
    fn snap_blit_opaque_rotate90() {
        // Capture used (sh, sw) for rotate90 blit dimensions
        let path = test_asset_path("sprite_test.gif");
        let (sprite, _) = load_gif_indexed(&path);
        let sh = sprite.height as i32;
        let sw = sprite.width as i32;
        run_snapshot_test(
            "sprite_test.gif",
            "blit_opaque_rotate90",
            16,
            16,
            Some((sh, sw)),
            (0, 0),
            BlitOrientation::Rotate90,
            BlitBlend::Copy,
            0,
        );
    }
    #[test]
    fn snap_blit_opaque_clipped() {
        run_snapshot_test(
            "sprite_test.gif",
            "blit_opaque_clipped",
            -16,
            -16,
            None,
            (0, 0),
            BlitOrientation::Normal,
            BlitBlend::Copy,
            0,
        );
    }
    #[test]
    fn snap_blit_opaque_colortable() {
        run_snapshot_test(
            "sprite_test.gif",
            "blit_opaque_colortable",
            16,
            16,
            None,
            (0, 0),
            BlitOrientation::Normal,
            BlitBlend::ColorTable,
            77,
        );
    }
    #[test]
    fn snap_blit_opaque_colortable_mx() {
        run_snapshot_test(
            "sprite_test.gif",
            "blit_opaque_colortable_mx",
            16,
            16,
            None,
            (0, 0),
            BlitOrientation::MirrorX,
            BlitBlend::ColorTable,
            77,
        );
    }
    #[test]
    fn snap_blit_opaque_subrect() {
        let path = test_asset_path("sprite_test.gif");
        let (sprite, _) = load_gif_indexed(&path);
        let sw = sprite.width as i32;
        let sh = sprite.height as i32;
        run_snapshot_test(
            "sprite_test.gif",
            "blit_opaque_subrect",
            16,
            16,
            Some((sw / 2, sh / 2)),
            (sw / 4, sh / 4),
            BlitOrientation::Normal,
            BlitBlend::Copy,
            0,
        );
    }
    #[test]
    fn snap_blit_transparent_identity() {
        run_snapshot_test(
            "sprite_transparent_test.gif",
            "blit_transparent_identity",
            16,
            16,
            None,
            (0, 0),
            BlitOrientation::Normal,
            BlitBlend::Copy,
            0,
        );
    }
    #[test]
    fn snap_blit_transparent_colortable() {
        run_snapshot_test(
            "sprite_transparent_test.gif",
            "blit_transparent_colortable",
            16,
            16,
            None,
            (0, 0),
            BlitOrientation::Normal,
            BlitBlend::ColorTable,
            77,
        );
    }
    #[test]
    fn snap_blit_opaque_additive() {
        run_snapshot_test(
            "sprite_test.gif",
            "blit_opaque_additive",
            16,
            16,
            None,
            (0, 0),
            BlitOrientation::Normal,
            BlitBlend::Additive,
            77,
        );
    }
    #[test]
    fn snap_blit_opaque_subtractive() {
        run_snapshot_test(
            "sprite_test.gif",
            "blit_opaque_subtractive",
            16,
            16,
            None,
            (0, 0),
            BlitOrientation::Normal,
            BlitBlend::Subtractive,
            0,
        );
    }
    #[test]
    fn snap_blit_transparent_additive() {
        run_snapshot_test(
            "sprite_transparent_test.gif",
            "blit_transparent_additive",
            16,
            16,
            None,
            (0, 0),
            BlitOrientation::Normal,
            BlitBlend::Additive,
            77,
        );
    }
    #[test]
    fn snap_blit_transparent_subtractive() {
        run_snapshot_test(
            "sprite_transparent_test.gif",
            "blit_transparent_subtractive",
            16,
            16,
            None,
            (0, 0),
            BlitOrientation::Normal,
            BlitBlend::Subtractive,
            0,
        );
    }

    // ---------------------------------------------------------------
    // Stippled blit snapshot tests
    // ---------------------------------------------------------------

    /// Run a stippled blit and compare against WA snapshot.
    fn run_stippled_snapshot_test(snap_name: &str, stipple_mode: u32, parity: u32) {
        let path = test_asset_path("sprite_transparent_test.gif");
        let (sprite, _) = load_gif_indexed(&path);
        let src = BlitSource::from(&sprite);
        let sw = sprite.width as i32;
        let sh = sprite.height as i32;

        let canvas_w = (sw + 32) as u32;
        let canvas_h = (sh + 32) as u32;
        let mut dst = PixelGrid::new(canvas_w, canvas_h);
        dst.data.fill(77); // background matching DLL capture

        blit_stippled(
            &mut dst.as_grid_mut(),
            &src,
            16,
            16,
            sw,
            sh,
            0,
            0,
            stipple_mode,
            parity,
        );

        assert_matches_snapshot(&dst, snap_name);
    }

    #[test]
    fn snap_stippled_mode0_par0() {
        run_stippled_snapshot_test("stippled_mode0_par0", 0, 0);
    }
    #[test]
    fn snap_stippled_mode0_par1() {
        run_stippled_snapshot_test("stippled_mode0_par1", 0, 1);
    }
    #[test]
    fn snap_stippled_mode1_par0() {
        run_stippled_snapshot_test("stippled_mode1_par0", 1, 0);
    }
    #[test]
    fn snap_stippled_mode1_par1() {
        run_stippled_snapshot_test("stippled_mode1_par1", 1, 1);
    }

    // ---------------------------------------------------------------
    // Tiled blit snapshot tests
    // ---------------------------------------------------------------

    /// Run a tiled blit and compare against WA snapshot.
    fn run_tiled_snapshot_test(snap_name: &str, flags: u32, bg_fill: u8) {
        let path = test_asset_path("sprite_transparent_test.gif");
        let (sprite, _) = load_gif_indexed(&path);
        let src = BlitSource::from(&sprite);
        let sw = sprite.width as i32;
        let sh = sprite.height as i32;

        // Match DLL capture: tile_w = sw * 4 + 32
        let canvas_w = (sw as u32) * 4 + 32;
        let canvas_h = (sh + 32) as u32;
        let mut dst = PixelGrid::new(canvas_w, canvas_h);
        dst.data.fill(bg_fill);

        let clip_left = 0i32;
        let clip_right = canvas_w as i32;

        blit_tiled(
            &mut dst.as_grid_mut(),
            &src,
            8, // initial_x matching DLL capture
            16,
            sw,
            sh,
            clip_left,
            clip_right,
            None,
            flags,
        );

        assert_matches_snapshot(&dst, snap_name);
    }

    #[test]
    fn snap_tiled_transparent() {
        run_tiled_snapshot_test("tiled_transparent", 0x0000_0001, 77);
    }
    #[test]
    fn snap_tiled_copy() {
        run_tiled_snapshot_test("tiled_copy", 0x0000_0000, 0);
    }
}
