use crate::task::Ptr32;

/// Render command entry (0x18 bytes).
///
/// Enqueued by DrawSpriteGlobal (type 4), DrawSpriteLocal (type 5),
/// and other drawing functions.
#[repr(C)]
pub struct RenderCommand {
    /// Command type: 4 = sprite global (world-space), 5 = sprite local (screen-space)
    pub command_type: u32,
    pub layer: u32,
    /// X position, upper 16 bits used (Fixed16 format)
    pub x_pos: u32,
    /// Y position, upper 16 bits used (Fixed16 format)
    pub y_pos: u32,
    pub sprite_id: u32,
    pub frame: u32,
}

const _: () = assert!(core::mem::size_of::<RenderCommand>() == 0x18);

/// Render queue — manages a downward-growing buffer of RenderCommand entries.
///
/// Passed as `this` (ECX) to DrawSpriteGlobal/Local. The buffer area sits
/// between offset 0x04 and the entry metadata at 0x10000+. Entries are
/// allocated from the end of the buffer downward (offset decrements by 0x18).
///
/// Max 0x800 (2048) entries per frame.
#[repr(C)]
pub struct RenderQueue {
    /// 0x00000: Buffer write offset (i32, decrements by 0x18 per entry)
    pub buffer_offset: i32,
    /// 0x00004 - 0x10003: Buffer area for RenderCommand entries
    pub _buffer: [u8; 0x10000],
    /// 0x10004: Number of enqueued entries (max 0x800)
    pub entry_count: u32,
    /// 0x10008: Pointer array — entry_ptrs[N] points to the N-th RenderCommand
    pub entry_ptrs: [Ptr32; 0x800],
}

impl RenderQueue {
    /// Allocate a new entry from the downward-growing buffer.
    ///
    /// Returns `None` if the queue is full (>= 0x800 entries or buffer exhausted).
    /// The returned reference is registered in `entry_ptrs` and `entry_count` is incremented.
    pub unsafe fn alloc_entry(&mut self) -> Option<&mut RenderCommand> {
        if self.entry_count >= 0x800 {
            return None;
        }
        let new_offset = self.buffer_offset - core::mem::size_of::<RenderCommand>() as i32;
        if new_offset < 0 {
            return None;
        }
        self.buffer_offset = new_offset;

        let entry = &mut *(self._buffer.as_mut_ptr().add(new_offset as usize)
            as *mut RenderCommand);
        self.entry_ptrs[self.entry_count as usize] = entry as *mut RenderCommand as u32;
        self.entry_count += 1;
        Some(entry)
    }
}

/// Render command type constants.
pub mod command_type {
    pub const DRAW_SPRITE_GLOBAL: u32 = 4;
    pub const DRAW_SPRITE_LOCAL: u32 = 5;
}
