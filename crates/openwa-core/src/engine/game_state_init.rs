//! Pure Rust implementations of DDGame__InitGameState sub-functions.
//!
//! Each function is hooked individually so it works regardless of whether
//! InitGameState itself is Rust or the original WA code.

use crate::wa_alloc::wa_malloc;

/// Pure Rust implementation of SpriteGfxTable__Init (0x541620).
///
/// Convention: fastcall(ECX=base, EDX=count), plain RET.
///
/// Initializes two parallel arrays:
/// - `base[0..count]`: identity permutation (index[i] = i)
/// - `base+0x2000[0..count]`: all 0xFFFFFFFF (unused markers)
/// Plus 3 trailer fields at +0x3000/+0x3004/+0x3008.
pub unsafe fn sprite_gfx_table_init(base: *mut u8, count: u32) {
    for i in 0..count {
        // Index table: base + i*4 = i
        *((base as *mut u32).add(i as usize)) = i;
        // Lookup table: base + 0x2000 + i*4 = 0xFFFFFFFF
        *((base.add(0x2000) as *mut u32).add(i as usize)) = 0xFFFF_FFFF;
    }
    *(base.add(0x3000) as *mut u32) = count;
    *(base.add(0x3004) as *mut u32) = 0;
    *(base.add(0x3008) as *mut u32) = count;
}

/// Pure Rust implementation of RingBuffer__Init (0x541060).
///
/// Convention: usercall(EAX=capacity, ESI=struct_ptr), plain RET.
///
/// Allocates a zero-filled buffer of `capacity` bytes (aligned + 0x20 header),
/// then initializes the ring buffer struct (7 DWORDs):
/// - [0]: data pointer
/// - [1]: capacity
/// - [2]-[6]: zeroed (head, tail, count, etc.)
pub unsafe fn ring_buffer_init(struct_ptr: *mut u8, capacity: u32) {
    let alloc_size = ((capacity + 3) & !3) + 0x20;
    let data = wa_malloc(alloc_size);
    if !data.is_null() {
        core::ptr::write_bytes(data, 0, capacity as usize);
    }

    let s = struct_ptr as *mut u32;
    *s.add(0) = data as u32; // data pointer
    *s.add(1) = capacity; // capacity
    *s.add(2) = 0; // field 2
    *s.add(3) = 0; // field 3
    *s.add(4) = 0; // field 4
    *s.add(5) = 0; // field 5
    *s.add(6) = 0; // field 6
}
