//! SpriteGfxTable — sprite allocation table embedded inside GameWorld.
//!
//! Two parallel u32 arrays plus a 3-DWORD trailer:
//! - `[0..count]` at `+0x0000`: identity permutation (slot index)
//! - `[0..count]` at `+0x2000`: lookup table (0xFFFFFFFF = unused)
//! - `+0x3000`: total count
//! - `+0x3004`: head/cursor
//! - `+0x3008`: free count

/// Pure Rust implementation of SpriteGfxTable__Init (0x541620).
///
/// Convention: fastcall(ECX=base, EDX=count), plain RET.
pub unsafe fn sprite_gfx_table_init(base: *mut u8, count: u32) {
    unsafe {
        for i in 0..count {
            *((base as *mut u32).add(i as usize)) = i;
            *((base.add(0x2000) as *mut u32).add(i as usize)) = 0xFFFFFFFF;
        }
        *(base.add(0x3000) as *mut u32) = count;
        *(base.add(0x3004) as *mut u32) = 0;
        *(base.add(0x3008) as *mut u32) = count;
    }
}
