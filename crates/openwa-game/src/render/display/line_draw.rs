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

use openwa_core::fixed::Fixed;

/// Trait for pixel-addressable surfaces used by drawing algorithms.
///
/// Implemented for `PixelGrid` (test-only, pure Rust) and can be implemented
/// for `DisplayBitGrid` (runtime, via vtable dispatch).
pub trait PixelWriter {
    fn put_pixel_clipped(&mut self, x: i32, y: i32, color: u8);
    /// Fill a horizontal span from x1 (inclusive) to x2 (exclusive) at row y.
    fn fill_hline(&mut self, x1: i32, x2: i32, y: i32, color: u8);
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

/// Borrowed mutable view into an 8bpp pixel buffer.
///
/// Same layout as `PixelGrid`, but `data` is a `&mut [u8]` instead of `Vec<u8>`.
/// Used when the buffer is owned externally (e.g., WA's BitGrid data) and wrapping
/// it in a `Vec` would be unsound. All blit functions accept this type.
pub struct PixelGridMut<'a> {
    pub data: &'a mut [u8],
    pub width: u32,
    pub height: u32,
    pub row_stride: u32,
    pub clip_left: u32,
    pub clip_top: u32,
    pub clip_right: u32,
    pub clip_bottom: u32,
}

impl<'a> PixelGridMut<'a> {
    /// Create a new `PixelGridMut` with a shorter lifetime, allowing the
    /// original to be reused afterward. Equivalent to `&mut *self` for
    /// a by-value type — needed when passing to functions that consume
    /// `PixelGridMut` inside a loop.
    pub fn reborrow(&mut self) -> PixelGridMut<'_> {
        PixelGridMut {
            data: self.data,
            width: self.width,
            height: self.height,
            row_stride: self.row_stride,
            clip_left: self.clip_left,
            clip_top: self.clip_top,
            clip_right: self.clip_right,
            clip_bottom: self.clip_bottom,
        }
    }
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

    /// Borrow this grid as a `PixelGridMut` — the type accepted by blit functions.
    pub fn as_grid_mut(&mut self) -> PixelGridMut<'_> {
        PixelGridMut {
            data: &mut self.data,
            width: self.width,
            height: self.height,
            row_stride: self.row_stride,
            clip_left: self.clip_left,
            clip_top: self.clip_top,
            clip_right: self.clip_right,
            clip_bottom: self.clip_bottom,
        }
    }

    /// Clear all pixels to zero.
    pub fn clear(&mut self) {
        self.data.fill(0);
    }

    /// Load a snapshot file.
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
    fn fill_hline(&mut self, x1: i32, x2: i32, y: i32, color: u8) {
        let offset = (y as u32 * self.row_stride + x1 as u32) as usize;
        let len = (x2 - x1) as usize;
        self.data[offset..offset + len].fill(color);
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

/// Clip a line segment to the writer's clip rectangle.
///
/// Port of 0x4F7150. All coordinates are Fixed-point (16.16).
/// Returns true if the clipped line is (partially) visible.
///
/// The clip rectangle is scaled from the writer's pixel bounds to Fixed.
fn clip_line(
    x1: &mut Fixed,
    y1: &mut Fixed,
    x2: &mut Fixed,
    y2: &mut Fixed,
    writer: &impl PixelWriter,
) -> bool {
    let cl = Fixed::from_int(writer.clip_left());
    let ct = Fixed::from_int(writer.clip_top());
    let cr = Fixed::from_int(writer.clip_right());
    let cb = Fixed::from_int(writer.clip_bottom());

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
        let ratio = (cl - *x1).div_raw(*x2 - *x1);
        *y1 += (*y2 - *y1).mul_raw(ratio);
        *x1 = cl;
    } else if *x1 > cr {
        let ratio = (cr - *x1).div_raw(*x2 - *x1);
        *y1 += (*y2 - *y1).mul_raw(ratio);
        *x1 = cr;
    }

    // Clip endpoint 2 against x bounds
    if *x2 < cl {
        let ratio = (cl - *x2).div_raw(*x1 - *x2);
        *y2 += (*y1 - *y2).mul_raw(ratio);
        *x2 = cl;
    } else if *x2 > cr {
        let ratio = (cr - *x2).div_raw(*x1 - *x2);
        *y2 += (*y1 - *y2).mul_raw(ratio);
        *x2 = cr;
    }

    // Check y visibility after x clipping
    if !(*y1 <= cb || *y2 <= cb) {
        return false;
    }
    if !(*y1 >= ct || *y2 >= ct) {
        return false;
    }

    // Clip endpoint 1 against y bounds
    if *y1 < ct {
        let ratio = (ct - *y1).div_raw(*y2 - *y1);
        *x1 += (*x2 - *x1).mul_raw(ratio);
        *y1 = ct;
    } else if *y1 > cb {
        let ratio = (cb - *y1).div_raw(*y2 - *y1);
        *x1 += (*x2 - *x1).mul_raw(ratio);
        *y1 = cb;
    }

    // Clip endpoint 2 against y bounds
    if *y2 < ct {
        let ratio = (ct - *y2).div_raw(*y1 - *y2);
        *x2 += (*x1 - *x2).mul_raw(ratio);
        *y2 = ct;
    } else if *y2 > cb {
        let ratio = (cb - *y2).div_raw(*y1 - *y2);
        *x2 += (*x1 - *x2).mul_raw(ratio);
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
fn raster_hmajor(
    writer: &mut impl PixelWriter,
    x1: Fixed,
    y1: Fixed,
    x2: Fixed,
    _y2: Fixed,
    color: u8,
) {
    let start = x1.pixel_center();
    let end = x2.pixel_center();
    if start == end {
        return;
    }

    let slope = (_y2 - y1).div_raw(x2 - x1);
    let mut y = y1; // raw Fixed y, NOT adjusted for sub-pixel offset
    let count = (end - start).to_int();

    for (px, _) in (start.to_int()..).zip(0..=count) {
        writer.put_pixel_clipped(px, y.round_to_int(), color);
        y += slope;
    }
}

/// Vertical-major DDA rasterizer (port of 0x4F7480).
///
/// Iterates from y1 to y2 (pixel-rounded), computing x via Fixed-point slope.
fn raster_vmajor(
    writer: &mut impl PixelWriter,
    x1: Fixed,
    y1: Fixed,
    _x2: Fixed,
    y2: Fixed,
    color: u8,
) {
    let start = y1.pixel_center();
    let end = y2.pixel_center();
    if start == end {
        return;
    }

    let slope = (_x2 - x1).div_raw(y2 - y1);
    let mut x = x1; // raw Fixed x
    let count = (end - start).to_int();

    for (py, _) in (start.to_int()..).zip(0..=count) {
        writer.put_pixel_clipped(x.round_to_int(), py, color);
        x += slope;
    }
}

/// Draw a single-color clipped line. Port of 0x4F7500.
pub fn draw_line_clipped(
    writer: &mut impl PixelWriter,
    mut x1: Fixed,
    mut y1: Fixed,
    mut x2: Fixed,
    mut y2: Fixed,
    color: u8,
) {
    let dx = (x2 - x1).abs();
    let dy = (y2 - y1).abs();

    if dx < dy {
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

/// Horizontal-major thick wide rasterizer (port of 0x4F77C0).
///
/// Draws 8 surrounding pixels per step around a 2x2 body center.
fn raster_thick_hmajor_wide(
    writer: &mut impl PixelWriter,
    x1: Fixed,
    y1: Fixed,
    x2: Fixed,
    y2: Fixed,
    color: u8,
) {
    let start = x1.pixel_center();
    let end = x2.pixel_center();
    if start == end {
        return;
    }

    let slope = (y2 - y1).div_raw(x2 - x1);
    let mut y = y1 + slope.mul_raw(start - x1);
    let count = (end - start).to_int();
    if count == 0 {
        return;
    }

    for (px, _) in (start.to_int()..).zip(0..count) {
        let py = y.to_int();
        writer.put_pixel_clipped(px, py - 1, color);
        writer.put_pixel_clipped(px + 1, py - 1, color);
        writer.put_pixel_clipped(px - 1, py, color);
        writer.put_pixel_clipped(px + 2, py, color);
        writer.put_pixel_clipped(px - 1, py + 1, color);
        writer.put_pixel_clipped(px + 2, py + 1, color);
        writer.put_pixel_clipped(px, py + 2, color);
        writer.put_pixel_clipped(px + 1, py + 2, color);

        y += slope;
    }
}

/// Horizontal-major thick narrow rasterizer (port of 0x4F75F0).
///
/// Draws 4 body pixels per step (2x2 center), overwriting the wide pass.
fn raster_thick_hmajor_narrow(
    writer: &mut impl PixelWriter,
    x1: Fixed,
    y1: Fixed,
    x2: Fixed,
    y2: Fixed,
    color: u8,
) {
    let start = x1.pixel_center();
    let end = x2.pixel_center();
    if start == end {
        return;
    }

    let slope = (y2 - y1).div_raw(x2 - x1);
    let mut y = y1 + slope.mul_raw(start - x1);
    let count = (end - start).to_int();
    if count == 0 {
        return;
    }

    for (px, _) in (start.to_int()..).zip(0..count) {
        let py = y.to_int();
        writer.put_pixel_clipped(px, py, color);
        writer.put_pixel_clipped(px + 1, py, color);
        writer.put_pixel_clipped(px, py + 1, color);
        writer.put_pixel_clipped(px + 1, py + 1, color);

        y += slope;
    }
}

/// Vertical-major thick wide rasterizer (port of 0x4F7910).
///
/// Transposed version — draws 8 surrounding pixels per step.
fn raster_thick_vmajor_wide(
    writer: &mut impl PixelWriter,
    x1: Fixed,
    y1: Fixed,
    x2: Fixed,
    y2: Fixed,
    color: u8,
) {
    let start = y1.pixel_center();
    let end = y2.pixel_center();
    if start == end {
        return;
    }

    let slope = (x2 - x1).div_raw(y2 - y1);
    let mut x = x1 + slope.mul_raw(start - y1);
    let count = (end - start).to_int();

    for (py, _) in (start.to_int()..).zip(0..count) {
        let px = x.to_int();
        writer.put_pixel_clipped(px, py - 1, color);
        writer.put_pixel_clipped(px + 1, py - 1, color);
        writer.put_pixel_clipped(px - 1, py, color);
        writer.put_pixel_clipped(px + 2, py, color);
        writer.put_pixel_clipped(px - 1, py + 1, color);
        writer.put_pixel_clipped(px + 2, py + 1, color);
        writer.put_pixel_clipped(px, py + 2, color);
        writer.put_pixel_clipped(px + 1, py + 2, color);

        x += slope;
    }
}

/// Vertical-major thick narrow rasterizer (port of 0x4F76D0).
///
/// Transposed version — draws 4 body pixels per step.
fn raster_thick_vmajor_narrow(
    writer: &mut impl PixelWriter,
    x1: Fixed,
    y1: Fixed,
    x2: Fixed,
    y2: Fixed,
    color: u8,
) {
    let start = y1.pixel_center();
    let end = y2.pixel_center();
    if start == end {
        return;
    }

    let slope = (x2 - x1).div_raw(y2 - y1);
    let mut x = x1 + slope.mul_raw(start - y1);
    let count = (end - start).to_int();

    for (py, _) in (start.to_int()..).zip(0..count) {
        let px = x.to_int();
        writer.put_pixel_clipped(px, py, color);
        writer.put_pixel_clipped(px + 1, py, color);
        writer.put_pixel_clipped(px, py + 1, color);
        writer.put_pixel_clipped(px + 1, py + 1, color);

        x += slope;
    }
}

/// Draw a two-color thick clipped line. Port of 0x4F7A60.
///
/// Draws a 2px-wide line. First pass draws 8 surrounding pixels in `color2`,
/// second pass overwrites 4 center pixels in `color1`.
pub fn draw_line_two_color(
    writer: &mut impl PixelWriter,
    mut x1: Fixed,
    mut y1: Fixed,
    mut x2: Fixed,
    mut y2: Fixed,
    color1: u8,
    color2: u8,
) {
    let dx = (x2 - x1).abs();
    let dy = (y2 - y1).abs();

    if dx < dy {
        // Vertical-major
        // Special case: zero horizontal extent → nudge x back by 1 pixel
        if dx == Fixed::ZERO {
            x1 -= Fixed::ONE;
            x2 -= Fixed::ONE;
        }
        if y1 > y2 {
            core::mem::swap(&mut x1, &mut x2);
            core::mem::swap(&mut y1, &mut y2);
        }
        if clip_line(&mut x1, &mut y1, &mut x2, &mut y2, writer) {
            raster_thick_vmajor_wide(writer, x1, y1, x2, y2, color2);
            raster_thick_vmajor_narrow(writer, x1, y1, x2, y2, color1);
        }
    } else {
        // Horizontal-major
        // Special case: zero vertical extent → nudge y back by 1 pixel
        if dy == Fixed::ZERO {
            y1 -= Fixed::ONE;
            y2 -= Fixed::ONE;
        }
        if x1 > x2 {
            core::mem::swap(&mut x1, &mut x2);
            core::mem::swap(&mut y1, &mut y2);
        }
        if clip_line(&mut x1, &mut y1, &mut x2, &mut y2, writer) {
            raster_thick_hmajor_wide(writer, x1, y1, x2, y2, color2);
            raster_thick_hmajor_narrow(writer, x1, y1, x2, y2, color1);
        }
    }
}

// =========================================================================
// Polygon fill (port of 0x4F7BA0 + 0x4F7D00 + 0x4F7E90)
// =========================================================================

/// Maximum vertices after clipping. Sutherland-Hodgman can at most double
/// the vertex count per clip edge, but in practice WA's global buffers are
/// ~256 entries. We use a generous limit.
const MAX_CLIP_VERTS: usize = 512;

/// A Fixed-point vertex (x, y).
#[derive(Clone, Copy)]
pub struct Vertex {
    pub x: Fixed,
    pub y: Fixed,
}

impl Vertex {
    #[inline]
    pub const fn new(x: Fixed, y: Fixed) -> Self {
        Self { x, y }
    }
}

/// Clip polygon edges against a single axis boundary.
///
/// Sutherland-Hodgman one-edge clip. For each consecutive edge (prev → curr),
/// outputs 0, 1, or 2 vertices depending on inside/outside transitions.
///
/// `inside` returns true if the coordinate is inside the boundary.
/// `intersect` computes the intersection point on the boundary.
fn clip_polygon_edge(
    input: &[Vertex],
    output: &mut [Vertex; MAX_CLIP_VERTS],
    inside: impl Fn(Fixed) -> bool,
    coord: impl Fn(&Vertex) -> Fixed,
    other: impl Fn(&Vertex) -> Fixed,
    make_vertex: impl Fn(Fixed, Fixed) -> Vertex,
    boundary: Fixed,
) -> usize {
    if input.len() < 2 {
        return 0;
    }
    let mut out_count = 0;

    let mut prev = input[input.len() - 1];
    for &curr in input {
        let prev_in = inside(coord(&prev));
        let curr_in = inside(coord(&curr));

        if prev_in && curr_in {
            // Both inside → emit current
            output[out_count] = curr;
            out_count += 1;
        } else if prev_in && !curr_in {
            // Leaving → emit intersection
            let ratio = (boundary - coord(&prev)).div_raw(coord(&curr) - coord(&prev));
            let clipped_other = other(&prev) + (other(&curr) - other(&prev)).mul_raw(ratio);
            output[out_count] = make_vertex(boundary, clipped_other);
            out_count += 1;
        } else if !prev_in && curr_in {
            // Entering → emit intersection then current
            let ratio = (boundary - coord(&prev)).div_raw(coord(&curr) - coord(&prev));
            let clipped_other = other(&prev) + (other(&curr) - other(&prev)).mul_raw(ratio);
            output[out_count] = make_vertex(boundary, clipped_other);
            out_count += 1;
            output[out_count] = curr;
            out_count += 1;
        }
        // else both outside → emit nothing

        if out_count >= MAX_CLIP_VERTS {
            break;
        }
        prev = curr;
    }
    out_count
}

/// Clip polygon against the writer's clip rectangle.
///
/// Port of 0x4F7BA0 (X clip) + 0x4F7D00 (Y clip).
/// Returns the clipped vertex count. Vertices are stored in `out`.
fn clip_polygon(
    verts: &[Vertex],
    out: &mut [Vertex; MAX_CLIP_VERTS],
    writer: &impl PixelWriter,
) -> usize {
    let cl = Fixed::from_int(writer.clip_left());
    let cr = Fixed::from_int(writer.clip_right());
    let ct = Fixed::from_int(writer.clip_top());
    let cb = Fixed::from_int(writer.clip_bottom());

    // Clip against left X
    let mut buf_a = [Vertex {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
    }; MAX_CLIP_VERTS];
    let count = clip_polygon_edge(
        verts,
        &mut buf_a,
        |x| x >= cl,
        |v| v.x,
        |v| v.y,
        |x, y| Vertex { x, y },
        cl,
    );
    if count < 3 {
        return 0;
    }

    // Clip against right X
    let mut buf_b = [Vertex {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
    }; MAX_CLIP_VERTS];
    let count = clip_polygon_edge(
        &buf_a[..count],
        &mut buf_b,
        |x| x <= cr,
        |v| v.x,
        |v| v.y,
        |x, y| Vertex { x, y },
        cr,
    );
    if count < 3 {
        return 0;
    }

    // Clip against top Y
    let count = clip_polygon_edge(
        &buf_b[..count],
        &mut buf_a,
        |y| y >= ct,
        |v| v.y,
        |v| v.x,
        |y, x| Vertex { x, y },
        ct,
    );
    if count < 3 {
        return 0;
    }

    // Clip against bottom Y
    clip_polygon_edge(
        &buf_a[..count],
        out,
        |y| y <= cb,
        |v| v.y,
        |v| v.x,
        |y, x| Vertex { x, y },
        cb,
    )
}

/// Maximum scanline height for the span table.
const MAX_SCANLINES: usize = 1024;

/// Rasterize a clipped convex polygon using scanline fill.
///
/// Port of 0x4F7E90. For each edge, computes a span table (X per scanline),
/// then fills horizontal lines between left and right span tables.
pub fn fill_polygon(writer: &mut impl PixelWriter, verts: &[Vertex], color: u8) {
    if verts.len() < 3 {
        return;
    }

    // Two span tables: one accumulates "left" edges, the other "right".
    // WA uses two global i16 arrays at 0x8AD058 and 0x8AD858.
    let mut span_a = [0i16; MAX_SCANLINES];
    let mut span_b = [0i16; MAX_SCANLINES];
    let mut y_min = i32::MAX;
    let mut y_max = i32::MIN;

    let n = verts.len();
    for i in 0..n {
        let prev = verts[(i + n - 1) % n];
        let curr = verts[i];

        let py_prev = prev.y.round_to_int();
        let py_curr = curr.y.round_to_int();

        if py_prev == py_curr {
            continue;
        }

        // Determine direction: iterate from smaller Y to larger Y
        let (start_x, start_y, end_x, end_y, py_start, py_end);
        if py_curr < py_prev {
            start_x = curr.x;
            start_y = curr.y;
            end_x = prev.x;
            end_y = prev.y;
            py_start = py_prev;
            py_end = py_curr;
        } else {
            start_x = prev.x;
            start_y = prev.y;
            end_x = curr.x;
            end_y = curr.y;
            py_start = py_curr;
            py_end = py_prev;
        };

        if py_end < y_min {
            y_min = py_end;
        }
        if py_start > y_max {
            y_max = py_start;
        }

        let scanline_count = py_start - py_end;
        if scanline_count <= 0 {
            continue;
        }

        // Compute DDA slope for X across scanlines (plain integer divide)
        let dx = end_x.to_raw() - start_x.to_raw();
        let dy = end_y.to_raw() - start_y.to_raw();
        let slope = if dy != 0 { dx / dy } else { 0 };

        // Starting X with sub-pixel correction
        let x_start = start_x.to_raw() + ((py_end * 0x10000 - start_y.to_raw()) + 0x8000) * slope;

        // Corrected step for the span table DDA
        let x_end_correction =
            ((py_start * 0x10000 - end_y.to_raw()) + 0x8000) * slope - x_start + end_x.to_raw();
        let step = if scanline_count != 0 {
            x_end_correction / scanline_count
        } else {
            0
        };

        // Fill the appropriate span table
        // Which table depends on edge direction (left vs right side of polygon).
        // WA uses two fixed tables offset by 0x800 bytes (1024 i16 entries).
        // We pick table based on the table pointer offset in the original:
        // 0x8AD058 vs 0x8AD858 — determined by the scanline pointer base.
        let table = if py_curr < py_prev {
            &mut span_a
        } else {
            &mut span_b
        };

        let mut val = x_start + 0x8000; // round
        for j in 0..scanline_count as usize {
            let idx = py_end as usize + j;
            if idx < MAX_SCANLINES {
                table[idx] = (val >> 16) as i16;
            }
            val = val.wrapping_add(step);
        }
    }

    // Fill scanlines between the two span tables
    for y in y_min..y_max {
        if y < 0 || y as usize >= MAX_SCANLINES {
            continue;
        }
        let a = span_a[y as usize] as i32;
        let b = span_b[y as usize] as i32;
        if a != b {
            let (x1, x2) = if a < b { (a, b) } else { (b, a) };
            writer.fill_hline(x1, x2, y, color);
        }
    }
}

/// Draw a filled polygon. Clips against the writer's bounds, then scanline-fills.
///
/// No heap allocation — uses stack buffers for clipping.
pub fn draw_polygon_filled(writer: &mut impl PixelWriter, vertices: &[Vertex], color: u8) {
    let mut clipped = [Vertex {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
    }; MAX_CLIP_VERTS];
    let count = clip_polygon(vertices, &mut clipped, writer);

    if count > 2 {
        fill_polygon(writer, &clipped[..count], color);
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

    fn f(x: i32) -> Fixed {
        Fixed::from_int(x)
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
        Fixed::from_raw(f(10).to_raw() + 0x8000),
        Fixed::from_raw(f(20).to_raw() + 0x4000),
        Fixed::from_raw(f(100).to_raw() + 0xC000),
        Fixed::from_raw(f(80).to_raw() + 0x2000),
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
        Fixed::from_raw(f(10).to_raw() + 0x8000),
        Fixed::from_raw(f(20).to_raw() + 0x4000),
        Fixed::from_raw(f(100).to_raw() + 0xC000),
        Fixed::from_raw(f(80).to_raw() + 0x2000),
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

    // Polygon fill snapshot tests
    fn v(x: Fixed, y: Fixed) -> Vertex {
        Vertex::new(x, y)
    }

    macro_rules! snapshot_test_poly {
        ($name:ident, $snap:expr, $verts:expr, $color:expr) => {
            #[test]
            fn $name() {
                let mut grid = PixelGrid::new(128, 128);
                draw_polygon_filled(&mut grid, $verts, $color);
                assert_matches_snapshot(&grid, $snap);
            }
        };
    }

    snapshot_test_poly!(
        snap_poly_triangle,
        "poly_triangle",
        &[v(f(64), f(10)), v(f(118), f(100)), v(f(10), f(100))],
        1
    );
    snapshot_test_poly!(
        snap_poly_square,
        "poly_square",
        &[
            v(f(20), f(20)),
            v(f(100), f(20)),
            v(f(100), f(100)),
            v(f(20), f(100))
        ],
        2
    );
    snapshot_test_poly!(
        snap_poly_diamond,
        "poly_diamond",
        &[
            v(f(64), f(10)),
            v(f(118), f(64)),
            v(f(64), f(118)),
            v(f(10), f(64))
        ],
        3
    );
    snapshot_test_poly!(
        snap_poly_partially_outside,
        "poly_partially_outside",
        &[v(f(64), f(-30)), v(f(160), f(100)), v(f(-30), f(100))],
        4
    );

    #[test]
    fn snap_poly_restricted_clip() {
        let mut grid = PixelGrid::new(128, 128);
        grid.clip_left = 30;
        grid.clip_top = 30;
        grid.clip_right = 98;
        grid.clip_bottom = 98;
        draw_polygon_filled(
            &mut grid,
            &[v(f(64), f(10)), v(f(118), f(100)), v(f(10), f(100))],
            5,
        );
        assert_matches_snapshot(&grid, "poly_restricted_clip");
    }
}
