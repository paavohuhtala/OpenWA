/// Pure-Rust 8bpp pixel grids shared by portable rendering helpers.
///
/// Row-major layout: `data[y * row_stride + x]`.
/// Row stride is 4-byte aligned by default (matching WA's BitGrid allocation).
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
    /// Create a reborrow with a shorter lifetime so the original can be reused.
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
    /// Create a new zeroed pixel grid with a full-surface clip rect.
    pub fn new(width: u32, height: u32) -> Self {
        let row_stride = (width + 3) & !3;
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

    /// Borrow this grid as a mutable view.
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
}
