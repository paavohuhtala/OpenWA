//! Line-drawing algorithms for 8bpp BitGrid surfaces.
//!
//! Pure Rust ports of WA's line drawing system:
//!
//! - `draw_line_clipped` (0x4F7500): Single-color DDA line with Cohen-Sutherland clipping.
//! - `draw_line_two_color` (0x4F7A60): Two-color thick (2px) line with clipping.
//!
//! ## Single-color architecture (0x4F7500)
//!
//! ```text
//! draw_line_clipped
//!   ├── Determine dominant axis (|dx| vs |dy|)
//!   ├── Sort endpoints in positive direction
//!   ├── clip_line() — Cohen-Sutherland, Fixed-point
//!   └── DDA rasterizer — 1 pixel per step (horizontal or vertical major)
//! ```

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

    /// Load a snapshot file and return (header, pixel_data).
    #[cfg(test)]
    pub fn from_snapshot(bytes: &[u8]) -> Self {
        let width = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let height = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let row_stride = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let data = bytes[12..].to_vec();
        Self {
            data,
            width,
            height,
            row_stride,
            clip_left: 0,
            clip_top: 0,
            clip_right: width,
            clip_bottom: height,
        }
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

/// Fixed-point divide: `(numerator << 16) / denominator`.
///
/// Port of FUN_005b3501. Used by the line clipper for endpoint interpolation.
#[inline]
fn fixed_div(numerator: i32, denominator: i32) -> i32 {
    if denominator == 0 {
        return 0;
    }
    (((numerator as i64) << 16) / denominator as i64) as i32
}

/// Fixed-point multiply-shift: `(a * b) >> 16`.
#[inline]
fn fixed_mul_shift(a: i32, b: i32) -> i32 {
    ((a as i64 * b as i64) >> 16) as i32
}

/// Clip a line segment to the writer's clip rectangle.
///
/// Port of 0x4F7150. All coordinates are Fixed-point (16.16).
/// Returns true if the clipped line is (partially) visible.
///
/// The clip rectangle is scaled from the writer's pixel bounds to Fixed.
fn clip_line(
    x1: &mut i32,
    y1: &mut i32,
    x2: &mut i32,
    y2: &mut i32,
    writer: &dyn PixelWriter,
) -> bool {
    let cl = writer.clip_left() << 16;
    let ct = writer.clip_top() << 16;
    let cr = writer.clip_right() << 16;
    let cb = writer.clip_bottom() << 16;

    // Cohen-Sutherland outcodes
    let mut code1 = 0u8;
    if *x1 < cl {
        code1 |= 1;
    }
    if *x1 > cr {
        code1 |= 2;
    }
    if *y1 < ct {
        code1 |= 4;
    }
    if *y1 > cb {
        code1 |= 8;
    }

    let mut code2 = 0u8;
    if *x2 < cl {
        code2 |= 1;
    }
    if *x2 > cr {
        code2 |= 2;
    }
    if *y2 < ct {
        code2 |= 4;
    }
    if *y2 > cb {
        code2 |= 8;
    }

    // Trivial reject
    if code1 & code2 != 0 {
        return false;
    }

    // Clip endpoint 1 against x bounds
    if *x1 < cl {
        let ratio = fixed_div(cl - *x1, *x2 - *x1);
        *y1 += fixed_mul_shift(*y2 - *y1, ratio);
        *x1 = cl;
    } else if *x1 > cr {
        let ratio = fixed_div(cr - *x1, *x2 - *x1);
        *y1 += fixed_mul_shift(*y2 - *y1, ratio);
        *x1 = cr;
    }

    // Clip endpoint 2 against x bounds
    if *x2 < cl {
        let ratio = fixed_div(cl - *x2, *x1 - *x2);
        *y2 += fixed_mul_shift(*y1 - *y2, ratio);
        *x2 = cl;
    } else if *x2 > cr {
        let ratio = fixed_div(cr - *x2, *x1 - *x2);
        *y2 += fixed_mul_shift(*y1 - *y2, ratio);
        *x2 = cr;
    }

    // Check y visibility after x clipping
    if (*y1 > cb || *y2 > cb) && (*y1 < ct || *y2 < ct) {
        // Spans both sides — skip (original logic)
    }
    if !(*y1 <= cb || *y2 <= cb) {
        return false;
    }
    if !(*y1 >= ct || *y2 >= ct) {
        return false;
    }

    // Clip endpoint 1 against y bounds
    if *y1 < ct {
        let ratio = fixed_div(ct - *y1, *y2 - *y1);
        *x1 += fixed_mul_shift(*x2 - *x1, ratio);
        *y1 = ct;
    } else if *y1 > cb {
        let ratio = fixed_div(cb - *y1, *y2 - *y1);
        *x1 += fixed_mul_shift(*x2 - *x1, ratio);
        *y1 = cb;
    }

    // Clip endpoint 2 against y bounds
    if *y2 < ct {
        let ratio = fixed_div(ct - *y2, *y1 - *y2);
        *x2 += fixed_mul_shift(*x1 - *x2, ratio);
        *y2 = ct;
    } else if *y2 > cb {
        let ratio = fixed_div(cb - *y2, *y1 - *y2);
        *x2 += fixed_mul_shift(*x1 - *x2, ratio);
        *y2 = cb;
    }

    // Final visibility check
    if (*x1 < cl || *x1 > cr) && (*x2 < cl || *x2 > cr) {
        return false;
    }

    true
}

// =========================================================================
// Single-color DDA line rasterizer (port of 0x4F7500)
// =========================================================================

/// Horizontal-major DDA rasterizer (port of 0x4F7400).
///
/// Iterates from x1 to x2 (pixel-rounded), computing y via Fixed-point slope.
fn raster_hmajor(writer: &mut dyn PixelWriter, x1: i32, y1: i32, x2: i32, y2: i32, color: u8) {
    let start = (x1 & !0xFFFF) + 0x8000; // round to pixel center
    let end = (x2 & !0xFFFF) + 0x8000;
    if start == end {
        return;
    }

    // slope = ((y2 - y1) << 16) / (x2 - x1), in 16.16 fixed
    let slope = fixed_div(y2 - y1, x2 - x1);
    let mut px = start >> 16;
    let mut y = y1; // starts at raw y1, NOT adjusted for sub-pixel offset
    let count = (end - start) >> 16;

    for _ in 0..=count {
        writer.put_pixel_clipped(px, (y + 0x8000) >> 16, color);
        y += slope;
        px += 1;
    }
}

/// Vertical-major DDA rasterizer (port of 0x4F7480).
///
/// Iterates from y1 to y2 (pixel-rounded), computing x via Fixed-point slope.
fn raster_vmajor(writer: &mut dyn PixelWriter, x1: i32, y1: i32, x2: i32, y2: i32, color: u8) {
    let start = (y1 & !0xFFFF) + 0x8000;
    let end = (y2 & !0xFFFF) + 0x8000;
    if start == end {
        return;
    }

    // slope = ((x2 - x1) << 16) / (y2 - y1)
    let slope = fixed_div(x2 - x1, y2 - y1);
    let mut py = start >> 16;
    let mut x = x1; // starts at raw x1
    let count = (end - start) >> 16;

    for _ in 0..=count {
        writer.put_pixel_clipped((x + 0x8000) >> 16, py, color);
        x += slope;
        py += 1;
    }
}

/// Draw a single-color clipped line. Port of 0x4F7500.
///
/// All coordinates are Fixed-point (16.16).
pub fn draw_line_clipped(
    writer: &mut dyn PixelWriter,
    mut x1: i32,
    mut y1: i32,
    mut x2: i32,
    mut y2: i32,
    color: u8,
) {
    let abs_dx = (x2 - x1).unsigned_abs();
    let abs_dy = (y2 - y1).unsigned_abs();

    if abs_dx < abs_dy {
        // Vertical-major: sort so y1 <= y2
        if y1 > y2 {
            core::mem::swap(&mut x1, &mut x2);
            core::mem::swap(&mut y1, &mut y2);
        }
        if clip_line(&mut x1, &mut y1, &mut x2, &mut y2, writer) {
            raster_vmajor(writer, x1, y1, x2, y2, color);
        }
    } else {
        // Horizontal-major: sort so x1 <= x2
        if x1 > x2 {
            core::mem::swap(&mut x1, &mut x2);
            core::mem::swap(&mut y1, &mut y2);
        }
        if clip_line(&mut x1, &mut y1, &mut x2, &mut y2, writer) {
            raster_hmajor(writer, x1, y1, x2, y2, color);
        }
    }
}

// =========================================================================
// Two-color thick line rasterizer (port of 0x4F7A60)
// =========================================================================

/// Horizontal-major thick outline rasterizer (port of 0x4F77C0).
///
/// Draws 8 outline pixels per step around a 2x2 body center.
fn raster_thick_hmajor_outline(
    writer: &mut dyn PixelWriter,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    color: u8,
) {
    let start = (x1 & !0xFFFF_i32) + 0x8000;
    let end = (x2 & !0xFFFF_i32) + 0x8000;
    if start == end {
        return;
    }

    // slope = ((y2 - y1) << 16) / (x2 - x1)
    let slope = fixed_div(y2 - y1, x2 - x1);
    // Sub-pixel y adjustment for pixel-center start
    let mut y = y1 + fixed_mul_shift(slope, start - x1);
    let mut px = start >> 16;
    let count = (end - start) >> 16;
    if count == 0 {
        return;
    }

    for _ in 0..count {
        let py = y >> 16;
        // 8 outline pixels around the 2x2 body
        writer.put_pixel_clipped(px, py - 1, color);
        writer.put_pixel_clipped(px + 1, py - 1, color);
        writer.put_pixel_clipped(px - 1, py, color);
        writer.put_pixel_clipped(px + 2, py, color);
        writer.put_pixel_clipped(px - 1, py + 1, color);
        writer.put_pixel_clipped(px + 2, py + 1, color);
        writer.put_pixel_clipped(px, py + 2, color);
        writer.put_pixel_clipped(px + 1, py + 2, color);

        y += slope;
        px += 1;
    }
}

/// Horizontal-major thick fill rasterizer (port of 0x4F75F0).
///
/// Draws 4 body pixels per step (2x2 center).
fn raster_thick_hmajor_fill(
    writer: &mut dyn PixelWriter,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    color: u8,
) {
    let start = (x1 & !0xFFFF_i32) + 0x8000;
    let end = (x2 & !0xFFFF_i32) + 0x8000;
    if start == end {
        return;
    }

    let slope = fixed_div(y2 - y1, x2 - x1);
    let mut y = y1 + fixed_mul_shift(slope, start - x1);
    let mut px = start >> 16;
    let count = (end - start) >> 16;
    if count == 0 {
        return;
    }

    for _ in 0..count {
        let py = y >> 16;
        // 2x2 body
        writer.put_pixel_clipped(px, py, color);
        writer.put_pixel_clipped(px + 1, py, color);
        writer.put_pixel_clipped(px, py + 1, color);
        writer.put_pixel_clipped(px + 1, py + 1, color);

        y += slope;
        px += 1;
    }
}

/// Vertical-major thick outline rasterizer (port of 0x4F7910).
///
/// Transposed version of the horizontal-major outline — draws 8 pixels per step.
fn raster_thick_vmajor_outline(
    writer: &mut dyn PixelWriter,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    color: u8,
) {
    let start = (y1 & !0xFFFF_i32) + 0x8000;
    let end = (y2 & !0xFFFF_i32) + 0x8000;
    if start == end {
        return;
    }

    let slope = fixed_div(x2 - x1, y2 - y1);
    let mut x = x1 + fixed_mul_shift(slope, start - y1);
    let mut py = start >> 16;
    let count = (end - start) >> 16;

    for _ in 0..count {
        let px = x >> 16;
        // 8 outline pixels (transposed pattern)
        writer.put_pixel_clipped(px, py - 1, color);
        writer.put_pixel_clipped(px + 1, py - 1, color);
        writer.put_pixel_clipped(px - 1, py, color);
        writer.put_pixel_clipped(px + 2, py, color);
        writer.put_pixel_clipped(px - 1, py + 1, color);
        writer.put_pixel_clipped(px + 2, py + 1, color);
        writer.put_pixel_clipped(px, py + 2, color);
        writer.put_pixel_clipped(px + 1, py + 2, color);

        x += slope;
        py += 1;
    }
}

/// Vertical-major thick fill rasterizer (port of 0x4F76D0).
///
/// Transposed version — draws 4 body pixels per step.
fn raster_thick_vmajor_fill(
    writer: &mut dyn PixelWriter,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    color: u8,
) {
    let start = (y1 & !0xFFFF_i32) + 0x8000;
    let end = (y2 & !0xFFFF_i32) + 0x8000;
    if start == end {
        return;
    }

    let slope = fixed_div(x2 - x1, y2 - y1);
    let mut x = x1 + fixed_mul_shift(slope, start - y1);
    let mut py = start >> 16;
    let count = (end - start) >> 16;

    for _ in 0..count {
        let px = x >> 16;
        // 2x2 body
        writer.put_pixel_clipped(px, py, color);
        writer.put_pixel_clipped(px + 1, py, color);
        writer.put_pixel_clipped(px, py + 1, color);
        writer.put_pixel_clipped(px + 1, py + 1, color);

        x += slope;
        py += 1;
    }
}

/// Draw a two-color thick clipped line. Port of 0x4F7A60.
///
/// All coordinates are Fixed-point (16.16). Draws a 2px-wide line with
/// `color1` as outline and `color2` as body fill.
pub fn draw_line_two_color(
    writer: &mut dyn PixelWriter,
    mut x1: i32,
    mut y1: i32,
    mut x2: i32,
    mut y2: i32,
    color1: u8,
    color2: u8,
) {
    let abs_dx = (x2 - x1).unsigned_abs();
    let abs_dy = (y2 - y1).unsigned_abs();

    if abs_dx < abs_dy {
        // Vertical-major
        // Special case: zero horizontal extent → nudge x coords back by 1 pixel
        if abs_dx == 0 {
            x1 -= 0x10000;
            x2 -= 0x10000;
        }
        // Sort so y1 <= y2
        if y1 > y2 {
            core::mem::swap(&mut x1, &mut x2);
            core::mem::swap(&mut y1, &mut y2);
        }
        if clip_line(&mut x1, &mut y1, &mut x2, &mut y2, writer) {
            // First pass: color2 wide (8 pixels/step), then color1 narrow (4 pixels/step)
            raster_thick_vmajor_outline(writer, x1, y1, x2, y2, color2);
            raster_thick_vmajor_fill(writer, x1, y1, x2, y2, color1);
        }
    } else {
        // Horizontal-major
        // Special case: zero vertical extent → nudge y coords back by 1 pixel
        if abs_dy == 0 {
            y1 -= 0x10000;
            y2 -= 0x10000;
        }
        // Sort so x1 <= x2
        if x1 > x2 {
            core::mem::swap(&mut x1, &mut x2);
            core::mem::swap(&mut y1, &mut y2);
        }
        if clip_line(&mut x1, &mut y1, &mut x2, &mut y2, writer) {
            // First pass: color2 wide (8 pixels/step), then color1 narrow (4 pixels/step)
            raster_thick_hmajor_outline(writer, x1, y1, x2, y2, color2);
            raster_thick_hmajor_fill(writer, x1, y1, x2, y2, color1);
        }
    }
}

// =========================================================================
// Tests
// =========================================================================

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

        grid.put_pixel_clipped(4, 4, 1);
        assert_eq!(grid.data[4 * grid.row_stride as usize + 4], 1);

        grid.put_pixel_clipped(12, 4, 2);
        assert_eq!(grid.data[4 * grid.row_stride as usize + 12], 0);

        grid.put_pixel_clipped(3, 4, 3);
        assert_eq!(grid.data[4 * grid.row_stride as usize + 3], 0);
    }

    fn f(x: i32) -> i32 {
        x << 16
    }

    fn load_snapshot(name: &str) -> PixelGrid {
        let path = format!(
            "{}/../../testdata/snapshots/{}.bin",
            env!("CARGO_MANIFEST_DIR"),
            name
        );
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("Failed to load snapshot {}: {} (path: {})", name, e, path));
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
            // Find first differing pixel for a helpful error message
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

    macro_rules! snapshot_test_clipped {
        ($name:ident, $snap:expr, $x1:expr, $y1:expr, $x2:expr, $y2:expr, $color:expr) => {
            #[test]
            fn $name() {
                let mut grid = PixelGrid::new(128, 128);
                draw_line_clipped(&mut grid, $x1, $y1, $x2, $y2, $color);
                assert_matches_snapshot(&grid, $snap);
            }
        };
    }

    snapshot_test_clipped!(
        snap_clipped_horizontal,
        "clipped_horizontal",
        f(10),
        f(64),
        f(118),
        f(64),
        1
    );
    snapshot_test_clipped!(
        snap_clipped_vertical,
        "clipped_vertical",
        f(64),
        f(10),
        f(64),
        f(118),
        2
    );
    snapshot_test_clipped!(
        snap_clipped_diagonal_45,
        "clipped_diagonal_45",
        f(10),
        f(10),
        f(118),
        f(118),
        3
    );
    snapshot_test_clipped!(
        snap_clipped_diagonal_steep,
        "clipped_diagonal_steep",
        f(60),
        f(10),
        f(68),
        f(118),
        4
    );
    snapshot_test_clipped!(
        snap_clipped_diagonal_shallow,
        "clipped_diagonal_shallow",
        f(10),
        f(60),
        f(118),
        f(68),
        5
    );
    snapshot_test_clipped!(
        snap_clipped_negative_slope,
        "clipped_negative_slope",
        f(118),
        f(10),
        f(10),
        f(118),
        6
    );
    snapshot_test_clipped!(
        snap_clipped_subpixel,
        "clipped_subpixel",
        f(10) + 0x8000,
        f(20) + 0x4000,
        f(100) + 0xC000,
        f(80) + 0x2000,
        7
    );
    snapshot_test_clipped!(
        snap_clipped_zero_length,
        "clipped_zero_length",
        f(64),
        f(64),
        f(64),
        f(64),
        8
    );
    snapshot_test_clipped!(
        snap_clipped_partially_outside,
        "clipped_partially_outside",
        f(-20),
        f(64),
        f(148),
        f(64),
        9
    );
    snapshot_test_clipped!(
        snap_clipped_fully_outside,
        "clipped_fully_outside",
        f(-50),
        f(-50),
        f(-10),
        f(-10),
        10
    );

    #[test]
    fn snap_clipped_restricted_clip() {
        let mut grid = PixelGrid::new(128, 128);
        grid.clip_left = 30;
        grid.clip_top = 30;
        grid.clip_right = 98;
        grid.clip_bottom = 98;
        draw_line_clipped(&mut grid, f(10), f(10), f(118), f(118), 11);
        assert_matches_snapshot(&grid, "clipped_restricted_clip");
    }

    // Two-color snapshot tests
    macro_rules! snapshot_test_twocol {
        ($name:ident, $snap:expr, $x1:expr, $y1:expr, $x2:expr, $y2:expr, $c1:expr, $c2:expr) => {
            #[test]
            fn $name() {
                let mut grid = PixelGrid::new(128, 128);
                draw_line_two_color(&mut grid, $x1, $y1, $x2, $y2, $c1, $c2);
                assert_matches_snapshot(&grid, $snap);
            }
        };
    }

    snapshot_test_twocol!(
        snap_twocol_horizontal,
        "twocol_horizontal",
        f(10),
        f(64),
        f(118),
        f(64),
        1,
        2
    );
    snapshot_test_twocol!(
        snap_twocol_vertical,
        "twocol_vertical",
        f(64),
        f(10),
        f(64),
        f(118),
        1,
        2
    );
    snapshot_test_twocol!(
        snap_twocol_diagonal_45,
        "twocol_diagonal_45",
        f(10),
        f(10),
        f(118),
        f(118),
        1,
        2
    );
    snapshot_test_twocol!(
        snap_twocol_steep,
        "twocol_steep",
        f(60),
        f(10),
        f(68),
        f(118),
        3,
        4
    );
    snapshot_test_twocol!(
        snap_twocol_shallow,
        "twocol_shallow",
        f(10),
        f(60),
        f(118),
        f(68),
        3,
        4
    );
    snapshot_test_twocol!(
        snap_twocol_negative,
        "twocol_negative",
        f(118),
        f(10),
        f(10),
        f(118),
        5,
        6
    );
    snapshot_test_twocol!(
        snap_twocol_subpixel,
        "twocol_subpixel",
        f(10) + 0x8000,
        f(20) + 0x4000,
        f(100) + 0xC000,
        f(80) + 0x2000,
        7,
        8
    );

    #[test]
    fn snap_twocol_restricted_clip() {
        let mut grid = PixelGrid::new(128, 128);
        grid.clip_left = 30;
        grid.clip_top = 30;
        grid.clip_right = 98;
        grid.clip_bottom = 98;
        draw_line_two_color(&mut grid, f(10), f(10), f(118), f(118), 9, 10);
        assert_matches_snapshot(&grid, "twocol_restricted_clip");
    }
}
