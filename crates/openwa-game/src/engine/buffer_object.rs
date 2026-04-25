//! BufferObject — dual-buffer container used by GameRuntime for state
//! serialization (`main_buffer`, `state_buffer`).
//!
//! WA's representation is 18 × u32 (0x48 bytes) with two parallel sub-buffers:
//! - `[0..5]`:  primary buffer header   (data, capacity, head, tail, world ptr)
//! - `[5..10]`: secondary buffer header (same shape)

use crate::engine::world::GameWorld;
use crate::engine::game_info::GameInfo;
use crate::wa_alloc::wa_malloc_zeroed;

/// Allocate a BufferObject (0x48 bytes) with both sub-buffer sizes computed
/// from `GameInfo`.
///
/// Pure Rust port of BufferObject__Constructor (0x545FD0). The original is
/// a usercall with the secondary buffer's capacity passed implicitly in EDI.
pub unsafe fn allocate_buffer_object(world: *mut GameWorld, game_info: *const GameInfo) -> *mut u8 {
    unsafe {
        let mem = wa_malloc_zeroed(0x48) as *mut u32;
        if mem.is_null() {
            return core::ptr::null_mut();
        }

        // Primary buffer
        let num_teams = (*game_info).num_teams_alloc as u32;
        let num_objects = (*game_info).object_slot_count;
        let buf1_capacity = num_teams * 0x450 + 0x4F178 + num_objects * 0x70;

        let buf1 = wa_malloc_zeroed(((buf1_capacity + 3) & !3) + 0x20);

        *mem.add(0) = buf1 as u32;
        *mem.add(1) = buf1_capacity;
        *mem.add(4) = world as u32;

        // Secondary buffer (capacity originally carried in EDI)
        let gi_raw = game_info as *const u8;
        let field_d9b4 = *gi_raw.add(0xD9B4);
        let field_d9b1 = *gi_raw.add(0xD9B1) as i8;
        let extra = if field_d9b4 != 0 && ((field_d9b1 as i32 - 2) as u32) >= 0x21 {
            0x190u32
        } else {
            0
        };
        let buf2_capacity = extra + 0x2DC;

        let buf2 = wa_malloc_zeroed(((buf2_capacity + 3) & !3) + 0x20);

        *mem.add(5) = buf2 as u32;
        *mem.add(6) = buf2_capacity;
        *mem.add(9) = world as u32;

        mem as *mut u8
    }
}
