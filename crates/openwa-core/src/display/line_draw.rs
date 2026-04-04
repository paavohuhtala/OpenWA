//! Line-drawing algorithms for 8bpp BitGrid surfaces.
//!
//! Pure Rust ports of WA's thick-line drawing system (0x4F7500 / 0x4F7A60).
//! The original draws 2-pixel-wide Bresenham-style lines with Fixed-point
//! Cohen-Sutherland clipping.
//!
//! ## Architecture
//!
//! ```text
//! draw_line_clipped / draw_line_two_color (outer dispatch)
//!   ├── Determine dominant axis (|dx| vs |dy|)
//!   ├── Sort endpoints in positive direction
//!   ├── clip_line() — Cohen-Sutherland, Fixed-point
//!   ├── Rasterizer (horizontal-major or vertical-major)
//!   │     └── put_pixel_clipped × 8 per step (outline)
//!   └── Fill rasterizer — 4 pixels per step (body color)
//! ```

use crate::fixed::Fixed;

/// Trait for pixel-addressable surfaces used by line-drawing algorithms.
///
/// Implemented for `PixelGrid` (test-only, pure Rust) and can be implemented
/// for `DisplayBitGrid` (runtime, via vtable dispatch).
pub trait PixelWriter {
    fn put_pixel_clipped(&mut self, x: i32, y: i32, color: u8);
    fn clip_left(&self) -> i32;
    fn clip_top(&self) -> i32;
    fn clip_right(&self) -> i32;
    fn clip_bottom(&self) -> i32;
}

/// Pure-Rust 8bpp pixel grid for unit testing. No vtable, no WA heap.
///
/// Row-major layout: `data[y * row_stride + x]`.
/// Row stride is 4-byte aligned (matching WA's BitGrid allocation).
pub struct PixelGrid {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub row_stride: u32,
    pub clip_left: u32,
    pub clip_top: u32,
    pub clip_right: u32,
    pub clip_bottom: u32,
}

impl PixelGrid {
    /// Create a new zeroed pixel grid with default clip rect covering the full surface.
    pub fn new(width: u32, height: u32) -> Self {
        let row_stride = (width + 3) & !3; // 4-byte aligned
        let size = (row_stride * height) as usize;
        Self {
            data: vec![0u8; size],
            width,
            height,
            row_stride,
            clip_left: 0,
            clip_top: 0,
            clip_right: width,
            clip_bottom: height,
        }
    }

    /// Clear all pixels to zero.
    pub fn clear(&mut self) {
        self.data.fill(0);
    }
}

impl PixelWriter for PixelGrid {
    #[inline]
    fn put_pixel_clipped(&mut self, x: i32, y: i32, color: u8) {
        if x >= self.clip_left as i32
            && x < self.clip_right as i32
            && y >= self.clip_top as i32
            && y < self.clip_bottom as i32
        {
            self.data[(y as u32 * self.row_stride + x as u32) as usize] = color;
        }
    }

    #[inline]
    fn clip_left(&self) -> i32 {
        self.clip_left as i32
    }
    #[inline]
    fn clip_top(&self) -> i32 {
        self.clip_top as i32
    }
    #[inline]
    fn clip_right(&self) -> i32 {
        self.clip_right as i32
    }
    #[inline]
    fn clip_bottom(&self) -> i32 {
        self.clip_bottom as i32
    }
}

// =========================================================================
// Fixed-point line clipping (port of 0x4F7150)
// =========================================================================

/// Cohen-Sutherland outcode bits for line clipping.
const LEFT: u8 = 1;
const RIGHT: u8 = 2;
const TOP: u8 = 4;
const BOTTOM: u8 = 8;

/// Compute Cohen-Sutherland outcode for a Fixed-point coordinate.
fn outcode(x: Fixed, y: Fixed, clip: &ClipRect) -> u8 {
    let mut code = 0u8;
    if x.to_raw() < clip.left.to_raw() {
        code |= LEFT;
    }
    if x.to_raw() > clip.right.to_raw() {
        code |= RIGHT;
    }
    if y.to_raw() < clip.top.to_raw() {
        code |= TOP;
    }
    if y.to_raw() > clip.bottom.to_raw() {
        code |= BOTTOM;
    }
    code
}

/// Clip rectangle in Fixed-point coordinates.
struct ClipRect {
    left: Fixed,
    top: Fixed,
    right: Fixed,
    bottom: Fixed,
}

/// Fixed-point multiply then shift: `(a * b) >> 16`.
///
/// Port of the pattern used throughout the WA line clipper.
#[inline]
fn fixed_mul_shift(a: i32, b: i32) -> i32 {
    ((a as i64 * b as i64) >> 16) as i32
}

/// Clip a line segment to the clip rectangle.
///
/// Port of 0x4F7150. Returns true if the clipped line is visible.
/// Modifies coordinates in-place.
///
/// The clip rectangle is read from the PixelWriter's bounds, scaled to Fixed.
fn clip_line(
    x1: &mut Fixed,
    y1: &mut Fixed,
    x2: &mut Fixed,
    y2: &mut Fixed,
    writer: &dyn PixelWriter,
) -> bool {
    let clip = ClipRect {
        left: Fixed::from_int(writer.clip_left()),
        top: Fixed::from_int(writer.clip_top()),
        right: Fixed::from_int(writer.clip_right()),
        bottom: Fixed::from_int(writer.clip_bottom()),
    };

    let code1 = outcode(*x1, *y1, &clip);
    let code2 = outcode(*x2, *y2, &clip);

    // Trivial reject: both endpoints on same side
    if code1 & code2 != 0 {
        return false;
    }

    // Clip x1 against left/right bounds
    if x1.to_raw() < clip.left.to_raw() {
        let ratio = fixed_div(clip.left.to_raw() - x1.to_raw(), x2.to_raw() - x1.to_raw());
        y1.0 += fixed_mul_shift(y2.to_raw() - y1.to_raw(), ratio);
        *x1 = clip.left;
    } else if x1.to_raw() > clip.right.to_raw() {
        let ratio = fixed_div(clip.right.to_raw() - x1.to_raw(), x2.to_raw() - x1.to_raw());
        y1.0 += fixed_mul_shift(y2.to_raw() - y1.to_raw(), ratio);
        *x1 = clip.right;
    }

    // Clip x2 against left/right bounds
    if x2.to_raw() < clip.left.to_raw() {
        let ratio = fixed_div(clip.left.to_raw() - x2.to_raw(), x1.to_raw() - x2.to_raw());
        y2.0 += fixed_mul_shift(y1.to_raw() - y2.to_raw(), ratio);
        *x2 = clip.left;
    } else if x2.to_raw() > clip.right.to_raw() {
        let ratio = fixed_div(clip.right.to_raw() - x2.to_raw(), x1.to_raw() - x2.to_raw());
        y2.0 += fixed_mul_shift(y1.to_raw() - y2.to_raw(), ratio);
        *x2 = clip.right;
    }

    // Check y bounds after x clipping
    if y1.to_raw() > clip.bottom.to_raw() || y2.to_raw() > clip.bottom.to_raw() {
        return false;
    }
    if y1.to_raw() < clip.top.to_raw() && y2.to_raw() < clip.top.to_raw() {
        return false;
    }

    // Clip y1 against top/bottom bounds
    if y1.to_raw() < clip.top.to_raw() {
        let ratio = fixed_div(clip.top.to_raw() - y1.to_raw(), y2.to_raw() - y1.to_raw());
        x1.0 += fixed_mul_shift(x2.to_raw() - x1.to_raw(), ratio);
        *y1 = clip.top;
    } else if y1.to_raw() > clip.bottom.to_raw() {
        let ratio = fixed_div(
            clip.bottom.to_raw() - y1.to_raw(),
            y2.to_raw() - y1.to_raw(),
        );
        x1.0 += fixed_mul_shift(x2.to_raw() - x1.to_raw(), ratio);
        *y1 = clip.bottom;
    }

    // Clip y2 against top/bottom bounds
    if y2.to_raw() < clip.top.to_raw() {
        let ratio = fixed_div(clip.top.to_raw() - y2.to_raw(), y1.to_raw() - y2.to_raw());
        x2.0 += fixed_mul_shift(x1.to_raw() - x2.to_raw(), ratio);
        *y2 = clip.top;
    } else if y2.to_raw() > clip.bottom.to_raw() {
        let ratio = fixed_div(
            clip.bottom.to_raw() - y2.to_raw(),
            y1.to_raw() - y2.to_raw(),
        );
        x2.0 += fixed_mul_shift(x1.to_raw() - x2.to_raw(), ratio);
        *y2 = clip.bottom;
    }

    // Final check: both endpoints must be within bounds
    if (x1.to_raw() < clip.left.to_raw() || x1.to_raw() > clip.right.to_raw())
        && (x2.to_raw() < clip.left.to_raw() || x2.to_raw() > clip.right.to_raw())
    {
        return false;
    }

    true
}

/// Fixed-point divide used by the line clipper.
///
/// Port of FUN_005b3501: computes `(numerator << 16) / denominator`.
#[inline]
fn fixed_div(numerator: i32, denominator: i32) -> i32 {
    if denominator == 0 {
        return 0;
    }
    (((numerator as i64) << 16) / denominator as i64) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_grid_basic() {
        let mut grid = PixelGrid::new(16, 16);
        grid.put_pixel_clipped(5, 5, 0xFF);
        assert_eq!(grid.data[5 * grid.row_stride as usize + 5], 0xFF);

        // Out of bounds — should be no-op
        grid.put_pixel_clipped(-1, 5, 0xAA);
        grid.put_pixel_clipped(16, 5, 0xAA);
        grid.put_pixel_clipped(5, -1, 0xAA);
        grid.put_pixel_clipped(5, 16, 0xAA);
    }

    #[test]
    fn pixel_grid_clipping() {
        let mut grid = PixelGrid::new(16, 16);
        grid.clip_left = 4;
        grid.clip_top = 4;
        grid.clip_right = 12;
        grid.clip_bottom = 12;

        // Inside clip rect — should write
        grid.put_pixel_clipped(4, 4, 1);
        assert_eq!(grid.data[4 * grid.row_stride as usize + 4], 1);

        // At clip boundary (exclusive) — should NOT write
        grid.put_pixel_clipped(12, 4, 2);
        assert_eq!(grid.data[4 * grid.row_stride as usize + 12], 0);

        // Outside clip rect — should NOT write
        grid.put_pixel_clipped(3, 4, 3);
        assert_eq!(grid.data[4 * grid.row_stride as usize + 3], 0);
    }
}
