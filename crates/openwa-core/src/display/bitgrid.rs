//! BitGrid — 2D pixel/bit buffer used for rendering layers and spatial queries.
//!
//! Two vtables:
//! - **Base** (0x6640EC): Bitfield grid for spatial queries (collision, visibility).
//!   Initialized by `BitGrid::init` (0x4F6370). Size: 0x2C bytes.
//! - **Display layer** (0x664144, 8 slots): Pixel buffer with drawing primitives.
//!   Used as rendering layers in DisplayGfx (+0x3D9C/+0x3DA0/+0x3DA4).
//!   Allocated at 0x4C bytes; first 0x2C bytes share the base layout.
//!
//! ## Field reinterpretation (display-layer context)
//!
//! Some fields have different semantics when used as a display layer:
//! - `cells_per_unit` (+0x0C) → bit depth (8 for 8bpp)
//! - `width/height` (+0x14/+0x18) → unused
//! - `_unused_1c/_unused_20` (+0x1C/+0x20) → clip_left / clip_top
//! - `width_dup/height_dup` (+0x24/+0x28) → clip_right / clip_bottom

use crate::address::va;
use crate::rebase::rb;
use crate::wa_alloc::wa_malloc;
use crate::FieldRegistry;

/// BitGrid — 2D pixel/bit buffer.
///
/// Base vtable at 0x6640EC (bitfield spatial grid).
/// Display-layer vtable at 0x664144 (pixel drawing).
/// Size: 0x2C bytes (base), 0x4C bytes (display layer).
#[derive(FieldRegistry)]
#[repr(C)]
pub struct BitGrid {
    pub vtable: u32,
    /// 0x04: External buffer flag. 0 = BitGrid owns data (frees on destroy),
    /// nonzero = external ownership.
    pub external_buffer: u32,
    /// 0x08: Pixel/bit data pointer
    pub data: *mut u8,
    /// 0x0C: Cells per unit (spatial) / bit depth (display, typically 8)
    pub cells_per_unit: u32,
    /// 0x10: Row stride in bytes
    pub row_stride: u32,
    /// 0x14: Width in pixels/cells
    pub width: u32,
    /// 0x18: Height in pixels/cells
    pub height: u32,
    /// 0x1C: Clip left / x minimum (display), init 0 (spatial)
    pub clip_left: u32,
    /// 0x20: Clip top / y minimum (display), init 0 (spatial)
    pub clip_top: u32,
    /// 0x24: Clip right / x maximum (display), init = width (spatial)
    pub clip_right: u32,
    /// 0x28: Clip bottom / y maximum (display), init = height (spatial)
    pub clip_bottom: u32,
}

const _: () = assert!(core::mem::size_of::<BitGrid>() == 0x2C);

impl BitGrid {
    /// Pure Rust implementation of BitGrid__Init (0x4F6370).
    ///
    /// Allocates a bit-per-cell grid buffer. `cells_per_unit` is typically 1.
    /// `width` and `height` are pixel dimensions. The buffer is a row-major
    /// bitfield with rows aligned to 4 bytes.
    ///
    /// # Safety
    /// `this` must point to a zero-filled allocation of at least 0x2C bytes.
    pub unsafe fn init(this: *mut BitGrid, cells_per_unit: u32, width: u32, height: u32) {
        let bits = cells_per_unit.wrapping_mul(width).wrapping_add(7) as i32;
        let row_stride = ((bits >> 3) + 3) & !3;
        let total_size = row_stride as u32 * height;

        let alloc_size = ((total_size + 3) & !3) + 0x20;
        let buffer = wa_malloc(alloc_size);

        if buffer.is_null() {
            return;
        }
        if total_size as usize > alloc_size as usize {
            return;
        }

        // Note: the original calls memset twice on the same buffer (possibly a
        // debug/paranoia pattern from MSVC 2005). We only do it once since the
        // second write is a no-op that modern compilers would eliminate anyway.
        core::ptr::write_bytes(buffer, 0, total_size as usize);

        (*this).vtable = rb(va::BIT_GRID_VTABLE);
        (*this).external_buffer = 0;
        (*this).data = buffer;
        (*this).cells_per_unit = cells_per_unit;
        (*this).row_stride = row_stride as u32;
        (*this).width = width;
        (*this).height = height;
        (*this).clip_left = 0;
        (*this).clip_top = 0;
        (*this).clip_right = width;
        (*this).clip_bottom = height;
    }
}

/// BitGrid display-layer vtable (0x664144, 8 slots).
///
/// Pixel buffer operations for 8-bit paletted rendering layers.
/// All pixel addressing uses: `data[y * row_stride + x]`
///
/// DDDisplay creates 3 layer objects in DDDisplay__Init:
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
