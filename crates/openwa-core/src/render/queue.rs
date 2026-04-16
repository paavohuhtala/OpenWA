use super::message::{COMMAND_TYPE_TYPED, RenderMessage, TypedRenderCmd};

/// Render queue — manages a downward-growing arena buffer of draw commands.
///
/// Passed as `this` (ECX) to the original DrawSprite*/DrawRect/etc. enqueue
/// functions. The buffer area sits between offset 0x04 and the entry metadata
/// at 0x10000+. Entries are allocated from the end of the buffer downward.
///
/// Max 0x800 (2048) entries per frame.
#[repr(C)]
pub struct RenderQueue {
    /// 0x00000: Buffer write offset (i32, decrements per allocation)
    pub buffer_offset: i32,
    /// 0x00004 - 0x10003: Buffer area for command entries (64KB)
    pub _buffer: [u8; 0x10000],
    /// 0x10004: Number of enqueued entries (max 0x800)
    pub entry_count: u32,
    /// 0x10008: Pointer array — entry_ptrs[N] points to the N-th command
    pub entry_ptrs: [*mut u8; 0x800],
}

impl RenderQueue {
    /// Allocate a command of type `T` from the downward-growing buffer.
    ///
    /// Returns `None` if the queue is full (>= 0x800 entries or buffer exhausted).
    /// The returned reference is registered in `entry_ptrs` and `entry_count` is incremented.
    pub unsafe fn alloc<T>(&mut self) -> Option<&mut T> {
        if self.entry_count >= 0x800 {
            return None;
        }
        let new_offset = self.buffer_offset - core::mem::size_of::<T>() as i32;
        if new_offset < 0 {
            return None;
        }
        self.buffer_offset = new_offset;

        let entry = &mut *(self._buffer.as_mut_ptr().add(new_offset as usize) as *mut T);
        self.entry_ptrs[self.entry_count as usize] = entry as *mut T as *mut u8;
        self.entry_count += 1;
        Some(entry)
    }

    /// Allocate `size` bytes from the buffer WITHOUT registering an entry pointer.
    ///
    /// Used for auxiliary data (vertex arrays, etc.) that will be referenced
    /// from a typed command via an explicit pointer. Both the aux data and
    /// the subsequent `push_typed()` command live in the same arena, freed
    /// together on frame reset.
    ///
    /// Unlike `alloc_raw()`, this does NOT consume an `entry_ptrs` slot or
    /// increment `entry_count`.
    pub unsafe fn alloc_aux(&mut self, size: usize) -> Option<*mut u8> {
        let new_offset = self.buffer_offset - size as i32;
        if new_offset < 0 {
            return None;
        }
        self.buffer_offset = new_offset;
        Some(self._buffer.as_mut_ptr().add(new_offset as usize))
    }

    /// Allocate `size` bytes from the downward-growing buffer.
    ///
    /// Like `alloc<T>()` but for variable-size entries (e.g. legacy commands
    /// still enqueued by unported WA.exe code).
    /// Returns `None` if the queue is full or buffer exhausted.
    pub unsafe fn alloc_raw(&mut self, size: usize) -> Option<*mut u8> {
        if self.entry_count >= 0x800 {
            return None;
        }
        let new_offset = self.buffer_offset - size as i32;
        if new_offset < 0 {
            return None;
        }
        self.buffer_offset = new_offset;

        let ptr = self._buffer.as_mut_ptr().add(new_offset as usize);
        self.entry_ptrs[self.entry_count as usize] = ptr;
        self.entry_count += 1;
        Some(ptr)
    }

    /// Enqueue a typed render message. Constant-time, zero allocation —
    /// the message is stored inline in the existing per-frame buffer.
    ///
    /// Returns `false` if the per-frame queue is full.
    ///
    /// # Safety
    ///
    /// The caller must ensure `self` points to a valid, live `RenderQueue`.
    pub unsafe fn push_typed(&mut self, layer: u32, message: RenderMessage) -> bool {
        match self.alloc::<TypedRenderCmd>() {
            Some(slot) => {
                *slot = TypedRenderCmd {
                    command_type: COMMAND_TYPE_TYPED,
                    layer,
                    message,
                };
                true
            }
            None => false,
        }
    }
}
