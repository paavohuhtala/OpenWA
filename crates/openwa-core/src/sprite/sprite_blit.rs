//! Sprite blitting algorithms for 8bpp BitGrid-like surfaces.
//!
//! Pure Rust port of WA's core blit pipeline (`FUN_004f6910`).
#![allow(clippy::too_many_arguments)]
use crate::pixel_grid::{PixelGrid, PixelGridMut};

/// Source surface for blit operations.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum BlitOrientation {
    Normal = 0,
    MirrorX = 1,
    MirrorY = 2,
    MirrorXY = 3,
    Rotate90 = 4,
    Rotate90MirrorX = 5,
    Rotate90MirrorY = 6,
    Rotate90MirrorXY = 7,
}

impl BlitOrientation {
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
    Copy,
    ColorTable,
    Additive,
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

    let dst_right = dst_x + width;
    let dst_bottom = dst_y + height;

    let clip_left = dst.clip_left as i32;
    let clip_top = dst.clip_top as i32;
    let clip_right = dst.clip_right as i32;
    let clip_bottom = dst.clip_bottom as i32;

    if dst_x >= clip_right
        || dst_right <= clip_left
        || dst_y >= clip_bottom
        || dst_bottom <= clip_top
    {
        return false;
    }

    let vis_left = dst_x.max(clip_left);
    let vis_right = dst_right.min(clip_right);
    let vis_top = dst_y.max(clip_top);
    let vis_bottom = dst_bottom.min(clip_bottom);

    if vis_left >= vis_right || vis_top >= vis_bottom {
        return false;
    }

    let vis_w = vis_right - vis_left;
    let vis_h = vis_bottom - vis_top;

    match blend {
        BlitBlend::Copy => {
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
                |s, d| s != 0 && d != 0,
            );
        }
        BlitBlend::Subtractive => {
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
                |s, d| s != 0 && d == 0,
            );
        }
    }

    true
}

const IDENTITY_TABLE: [u8; 256] = {
    let mut t = [0u8; 256];
    let mut i = 0;
    while i < 256 {
        t[i] = i as u8;
        i += 1;
    }
    t
};

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

    let clip_left = vis_left - dst_x;
    let clip_top = vis_top - dst_y;

    match orientation {
        Normal => (src_x + clip_left, src_y + clip_top),
        MirrorX => (src_x + width - 1 - clip_left, src_y + clip_top),
        MirrorY => (src_x + clip_left, src_y + height - 1 - clip_top),
        MirrorXY => (src_x + width - 1 - clip_left, src_y + height - 1 - clip_top),
        Rotate90 => (src_x + clip_top, src_y + clip_left),
        Rotate90MirrorX => (src_x + clip_top, src_y + width - 1 - clip_left),
        Rotate90MirrorY => (src_x + height - 1 - clip_top, src_y + clip_left),
        Rotate90MirrorXY => (src_x + height - 1 - clip_top, src_y + width - 1 - clip_left),
    }
}

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
            let sx = sx_start as usize;
            let src_slice = &src.data[src_row + sx..src_row + sx + vis_w as usize];
            dst.data[dst_row..dst_row + vis_w as usize].copy_from_slice(src_slice);
        } else {
            let mut sx = sx_start;
            for dx in 0..vis_w {
                dst.data[dst_row + dx as usize] = src.data[src_row + sx as usize];
                sx += sx_step;
            }
        }

        sy += sy_step;
    }
}

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

/// Create a PixelGrid from raw 8bpp indexed pixel data.
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

#[inline]
fn bit_get(data: &[u8], stride: u32, x: i32, y: i32) -> u8 {
    let byte = data[(y as u32 * stride + (x as u32 >> 3)) as usize];
    (byte >> (x & 7)) & 1
}

#[inline]
fn bit_put(data: &mut [u8], stride: u32, x: i32, y: i32, value: u8) {
    let idx = (y as u32 * stride + (x as u32 >> 3)) as usize;
    let bit = (x & 7) as u8;
    data[idx] = (data[idx] & !(1 << bit)) | (((value != 0) as u8) << bit);
}

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
            for row in clip_top..clip_bottom {
                let dst_off = row as usize * dst_stride as usize + dst_byte_x;
                let src_row = src_y + (row - clip_top);
                let src_off = src_row as usize * src_stride as usize + src_byte_x;
                dst[dst_off..dst_off + byte_width]
                    .copy_from_slice(&src[src_off..src_off + byte_width]);
            }
        }
        2 => {}
        _ => {
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

/// Blit a sprite with a checkerboard (stippled) pattern.
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

    let src_offset_x = src_x - dst_x;

    for row in 0..height {
        let sy = src_y + row;
        let dy = (dst_y - src_y) + sy;

        if dy < dst.clip_top as i32 || dy >= dst.clip_bottom as i32 {
            continue;
        }

        for col in 0..width {
            let dx = dst_x + col;

            if dx < dst.clip_left as i32 || dx >= dst.clip_right as i32 {
                continue;
            }

            if (dx as u32 ^ parity ^ dy as u32 ^ stipple_mode) & 1 == 0 {
                continue;
            }

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

/// Blit a sprite tiled horizontally across a destination region.
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

    let mut x = initial_x;
    while x < clip_left {
        x += tile_width;
    }
    while x > clip_left {
        x -= tile_width;
    }

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
