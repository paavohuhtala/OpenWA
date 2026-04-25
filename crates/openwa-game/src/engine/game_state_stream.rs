//! GameStateStream — DDGame's serialization sub-stream (DDGame.game_state_stream).
//!
//! Holds a 0x100-capacity primary buffer plus 32 × 0x100-capacity sub-streams.

use crate::wa_alloc::wa_malloc_zeroed;

/// Pure Rust port of GameStateStream__Init (0x4FB490).
///
/// Convention: stdcall(sub_object_ptr), plain RET.
///
/// Initializes the sub-object within GameStateStream:
/// - +0x14: capacity (0x100)
/// - +0x18/+0x1C: zeroed
/// - +0x20: main buffer (0x420 bytes, first 0x400 zeroed)
/// - +0x24..+0x224: 32 sub-buffer elements (each 0x10 bytes)
///
/// Each sub-buffer element (FUN_004fdc20):
/// - 0: capacity (0x100)
/// - 1/2: zeroed
/// - 3: buffer (0x420 bytes, first 0x400 zeroed)
pub unsafe fn game_state_stream_init(sub_obj: *mut u32) {
    unsafe {
        *sub_obj.add(5) = 0x100;
        *sub_obj.add(6) = 0;
        *sub_obj.add(7) = 0;

        let buf = wa_malloc_zeroed(0x420);
        *sub_obj.add(8) = buf as u32;

        for i in 0..32usize {
            let elem = sub_obj.add(9 + i * 4);
            *elem = 0x100;
            *elem.add(1) = 0;
            *elem.add(2) = 0;
            let elem_buf = wa_malloc_zeroed(0x420);
            *elem.add(3) = elem_buf as u32;
        }
    }
}
