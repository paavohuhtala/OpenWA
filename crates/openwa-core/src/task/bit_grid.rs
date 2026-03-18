//! BitGrid initialization — bitfield grid buffer for spatial queries.

use crate::rebase::rb;
use crate::wa_alloc::wa_malloc;

/// Bitfield grid buffer used for spatial collision/visibility queries.
/// Vtable at 0x6640EC (shared with DisplayGfx base).
/// Size: 0x2C bytes (11 u32 fields).
#[repr(C)]
pub struct BitGrid {
    pub vtable: u32,
    pub _unused_04: u32,
    pub data: *mut u8,
    pub cells_per_unit: u32,
    pub row_stride: u32,
    pub width: u32,
    pub height: u32,
    pub _unused_1c: u32,
    pub _unused_20: u32,
    pub width_dup: u32,
    pub height_dup: u32,
}

const _: () = assert!(core::mem::size_of::<BitGrid>() == 0x2C);

/// Pure Rust implementation of BitGrid__Init (0x4F6370).
///
/// Allocates a bit-per-cell grid buffer. `cells_per_unit` is typically 1.
/// `width` and `height` are pixel dimensions. The buffer is a row-major
/// bitfield with rows aligned to 4 bytes.
///
/// # Safety
/// `object` must point to a zero-filled allocation of at least 0x2C bytes.
pub unsafe fn bit_grid_init(object: *mut u8, cells_per_unit: u32, width: u32, height: u32) {
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

    // Memset twice (matches original — likely redundant but exact match)
    core::ptr::write_bytes(buffer, 0, total_size as usize);
    core::ptr::write_bytes(buffer, 0, total_size as usize);

    let tsm = &mut *(object as *mut BitGrid);
    tsm.vtable = rb(0x6640EC);
    tsm._unused_04 = 0;
    tsm.data = buffer;
    tsm.cells_per_unit = cells_per_unit;
    tsm.row_stride = row_stride as u32;
    tsm.width = width;
    tsm.height = height;
    tsm._unused_1c = 0;
    tsm._unused_20 = 0;
    tsm.width_dup = width;
    tsm.height_dup = height;
}
