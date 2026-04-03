//! BitGrid display layer vtable — pixel drawing primitives for rendering layers.
//!
//! The BitGrid struct lives in `crate::task::bit_grid`. This module adds the
//! display-layer vtable (0x664144, 8 slots) used by the three rendering layers
//! in DisplayGfx (+0x3D9C, +0x3DA0, +0x3DA4).
//!
//! When used as display layers, the BitGrid is allocated at 0x4C bytes (vs the
//! base 0x2C). The first 0x2C bytes share the same layout; the extra 0x20 bytes
//! hold display-layer-specific state.
//!
//! ## Field reinterpretation
//!
//! Some fields have different semantics in display-layer context:
//! - `cells_per_unit` (+0x0C) → bit depth (8 for 8bpp)
//! - `width/height` (+0x14/+0x18) → unused in display context
//! - `_unused_1c/20` (+0x1C/+0x20) → clip_left / clip_top
//! - `width_dup/height_dup` (+0x24/+0x28) → clip_right / clip_bottom

pub use crate::task::bit_grid::BitGrid;

/// BitGrid display-layer vtable (0x664144, 8 slots).
///
/// Pixel buffer operations for 8-bit paletted rendering layers.
/// All pixel addressing uses: `data[y * row_stride + x]`
///
/// The DDDisplay creates 3 layer objects in DDDisplay__Init:
/// - Layer 0 at DisplayGfx+0x3D9C
/// - Layer 1 at DisplayGfx+0x3DA0
/// - Layer 2 at DisplayGfx+0x3DA4 (uses BitGrid__Init for extended setup)
#[openwa_core::vtable(size = 8, va = 0x0066_4144, class = "BitGridDisplay")]
pub struct BitGridDisplayVtable {
    /// fill rectangle — memset rows [y1..y2) from x1 to x2 with color (0x4F9090, RET 0x14)
    #[slot(0)]
    pub fill_rect: fn(this: *mut BitGrid, x1: i32, y1: i32, x2: i32, y2: i32, color: u8),
    /// fill horizontal line — memset row y from x1 to x2 with color (0x4F90E0, RET 0x10)
    #[slot(1)]
    pub fill_hline: fn(this: *mut BitGrid, x1: i32, x2: i32, y: i32, color: u8),
    /// fill vertical line — set pixels in column x from y1 to y2 (0x4F9110, RET 0x10)
    #[slot(2)]
    pub fill_vline: fn(this: *mut BitGrid, x: i32, y1: i32, y2: i32, color: u8),
    /// destructor — reverts vtable to 0x6640EC, frees data if owned (0x4F5DE0, RET 0x4)
    #[slot(3)]
    pub destructor: fn(this: *mut BitGrid, flags: u8) -> *mut BitGrid,
    /// get pixel (clipped) — returns 0 if (x, y) is outside clip rect (0x4F9140, RET 0x8)
    ///
    /// Clip rect: `_unused_1c`..`width_dup` (x), `_unused_20`..`height_dup` (y)
    #[slot(4)]
    pub get_pixel_clipped: fn(this: *mut BitGrid, x: i32, y: i32) -> u8,
    /// put pixel (clipped) — no-op if (x, y) is outside clip rect (0x4F9180, RET 0xC)
    ///
    /// This is the main rendering primitive — DDDisplay dispatches drawing
    /// operations through these layer objects.
    #[slot(5)]
    pub put_pixel_clipped: fn(this: *mut BitGrid, x: i32, y: i32, color: u8),
    /// get pixel (unchecked) — direct read, no bounds checking (0x4F5E20, RET 0x8)
    #[slot(6)]
    pub get_pixel: fn(this: *mut BitGrid, x: i32, y: i32) -> u8,
    /// put pixel (unchecked) — direct write, no bounds checking (0x4F5E40, RET 0xC)
    #[slot(7)]
    pub put_pixel: fn(this: *mut BitGrid, x: i32, y: i32, color: u8),
}

// Note: no bind macro here because BitGrid.vtable is u32, not a typed pointer.
// Use vcall! or manual vtable dispatch for calling these methods.
