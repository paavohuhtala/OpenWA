//! Ring buffer primitives used by DDGameWrapper sub-objects.
//!
//! WA represents these as an opaque 7-DWORD struct:
//! - `[0]`: data pointer
//! - `[1]`: capacity
//! - `[2..7]`: head/tail/count fields (all zeroed at init)

use crate::wa_alloc::{wa_malloc, wa_malloc_zeroed};

/// Pure Rust implementation of RingBuffer__Init (0x541060).
///
/// Convention: usercall(EAX=capacity, ESI=struct_ptr), plain RET.
///
/// Allocates a zero-filled buffer of `capacity` bytes (rounded up to 4 + 0x20 header)
/// and writes the data pointer + capacity into the struct.
pub unsafe fn ring_buffer_init(struct_ptr: *mut u8, capacity: u32) {
    unsafe {
        let alloc_size = ((capacity + 3) & !3) + 0x20;
        let data = wa_malloc_zeroed(alloc_size);

        let s = struct_ptr as *mut u32;
        *s.add(0) = data as u32;
        *s.add(1) = capacity;
        *s.add(2) = 0;
        *s.add(3) = 0;
        *s.add(4) = 0;
        *s.add(5) = 0;
        *s.add(6) = 0;
    }
}

/// Allocate a raw ring-buffer-like object with manual field initialization.
/// Used for objects of struct sizes 0x3C/0x48 with various capacities.
pub unsafe fn allocate_ring_buffer_raw(alloc_size: u32, capacity: u32) -> *mut u8 {
    unsafe {
        let mem = wa_malloc_zeroed(alloc_size) as *mut u32;
        if mem.is_null() {
            return core::ptr::null_mut();
        }
        *mem.add(1) = capacity;
        let buf = wa_malloc(capacity + 0x20);
        core::ptr::write_bytes(buf, 0, capacity as usize);
        *mem = buf as u32;
        *mem.add(6) = 0;
        *mem.add(5) = 0;
        *mem.add(4) = 0;
        *mem.add(3) = 0;
        *mem.add(2) = 0;

        mem as *mut u8
    }
}

/// Allocate a 0x3C-byte RingBuffer wrapper (capacity 0x2000) using `ring_buffer_init`.
/// Used for the conditional network ring buffer.
pub unsafe fn allocate_ring_buffer_init() -> *mut u8 {
    unsafe {
        let mem = wa_malloc_zeroed(0x3C);
        if mem.is_null() {
            return core::ptr::null_mut();
        }
        ring_buffer_init(mem, 0x2000);
        mem
    }
}
