//! TaskStateMachine initialization — bitfield grid buffer for spatial queries.

use crate::rebase::rb;
use crate::wa_alloc::wa_malloc;

/// Pure Rust implementation of TaskStateMachine__Init (0x4F6370).
///
/// Convention: usercall(ESI=object, ECX=param1, EDI=height) + 1 stack(width), RET 0x4.
///
/// Allocates a bit-per-cell grid buffer. `param1` is typically 1 (cells per unit).
/// `width` and `height` are pixel dimensions. The buffer is a row-major bitfield
/// with rows aligned to 4 bytes.
///
/// # Safety
/// `object` must point to a zero-filled allocation of at least 0x2C bytes.
pub unsafe fn task_state_machine_init(object: *mut u8, param1: u32, width: u32, height: u32) {
    // Row stride: bits-to-bytes rounded up, then aligned to 4
    let bits = param1.wrapping_mul(width).wrapping_add(7) as i32;
    let row_stride = ((bits >> 3) + 3) & !3;
    let total_size = row_stride as u32 * height;

    // Allocate data buffer with 0x20-byte header
    let alloc_size = ((total_size + 3) & !3) + 0x20;
    let buffer = wa_malloc(alloc_size);

    if buffer.is_null() {
        return;
    }
    // Guard against integer overflow producing tiny alloc_size with huge total_size
    if total_size as usize > alloc_size as usize {
        return;
    }

    // Memset twice (matches original — likely redundant but exact match)
    core::ptr::write_bytes(buffer, 0, total_size as usize);
    core::ptr::write_bytes(buffer, 0, total_size as usize);

    let obj = object as *mut u32;
    *obj.add(0) = rb(0x6640EC); // vtable
    *obj.add(1) = 0;
    *obj.add(2) = buffer as u32; // data pointer
    *obj.add(3) = param1;
    *obj.add(4) = row_stride as u32;
    *obj.add(5) = width;
    *obj.add(6) = height;
    *obj.add(7) = 0;
    *obj.add(8) = 0;
    *obj.add(9) = width; // duplicate
    *obj.add(10) = height; // duplicate
}
