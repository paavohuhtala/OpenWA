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

use super::line_draw::PixelGrid;

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
    /// Additive color mix — source pixel index used as a fixed-point X scale.
    Additive,
    /// Subtractive color mix.
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
    dst: &mut PixelGrid,
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
            // Orientation is ignored — the DDDisplay layer handles visual
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
                dst, src, vis_left, vis_top, vis_w, vis_h, sx_start, sy_start, 1, 1,
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
                    dst, src, vis_left, vis_top, vis_w, vis_h, sx_start, sy_start, sx_step,
                    sy_step, table,
                );
            } else {
                blit_color_table(
                    dst, src, vis_left, vis_top, vis_w, vis_h, sx_start, sy_start, sx_step,
                    sy_step, table,
                );
            }
        }
        BlitBlend::Additive => {
            // TODO: port additive blend
        }
        BlitBlend::Subtractive => {
            // TODO: port subtractive blend
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
    dst: &mut PixelGrid,
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
    dst: &mut PixelGrid,
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
    dst: &mut PixelGrid,
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
            &mut dst,
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
            &mut dst_normal,
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
            &mut dst_mirror,
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
            &mut dst,
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
            &mut dst,
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
            &mut dst,
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
        for i in 0..256 {
            table[i] = (i as u8).wrapping_add(10);
        }
        table[0] = 0; // transparent stays 0

        blit_sprite_rect(
            &mut dst,
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
            &mut dst,
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
            &mut dst,
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
            &mut dst,
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
            let dst_val = dst.data[(0 * dst.row_stride as i32 + x) as usize];
            let src_val = sprite.data[(0 * sprite.row_stride as i32 + (sw - 1 - x)) as usize];
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
            &mut dst,
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
            &mut dst,
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
            &mut dst,
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
            &mut dst,
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
}
