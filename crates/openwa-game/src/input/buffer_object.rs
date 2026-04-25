//! Partial layout of `BufferObject` — the ring/message buffer used by
//! `GameRuntime` for input dispatch, serialization, and the state buffer.
//!
//! Full layout is unknown; only the fields touched by `classify_input_msg`
//! (0x00541100) and `alloc_slot` (0x004F9330) are typed here. The rest is
//! left as opaque padding so the struct is not yet sized.

use crate::FieldRegistry;

/// A single message node in `BufferObject::queue_head`.
///
/// Node layout (header + inline payload):
/// - `+0x00` `size` — `4 + payload_bytes` (4 accounts for `msg_type` being
///   counted as part of the "body" by the classifier).
/// - `+0x04` `next` — linked-list forward pointer (null = tail).
/// - `+0x08` `msg_type` — discriminator, written by the caller.
/// - `+0x0C` payload, `size - 4` bytes, written by the caller.
///
/// The allocator returns a pointer to `msg_type` so callers can write
/// `[msg_type, ...payload]` contiguously.
#[repr(C)]
pub struct BufferMsgNode {
    pub size: u32,
    pub next: *mut BufferMsgNode,
    pub msg_type: u32,
    // payload follows inline
}

/// Partial view of `BufferObject` — only the ring/queue fields used by
/// `classify_input_msg` and `alloc_slot` are typed.
#[derive(FieldRegistry)]
#[repr(C)]
pub struct BufferObject {
    /// 0x00: Start pointer of the backing ring (raw bytes).
    pub ring_start: *mut u8,
    /// 0x04: Total capacity of the backing ring, in bytes.
    pub capacity: u32,
    /// 0x08: Write cursor (offset from `ring_start`). Advanced by every
    /// successful `alloc_slot`. When the queue fully drains, `read_offset`
    /// re-syncs to this value.
    pub tail_offset: u32,
    /// 0x0C: Read cursor (offset from `ring_start`).
    pub read_offset: u32,
    /// 0x10: Last-allocated (tail) message node — linked forward from the
    /// previous tail on the next `alloc_slot`. Null when queue is empty.
    pub tail_node: *mut BufferMsgNode,
    /// 0x14: Head of the pending-message linked list. Null when empty.
    pub queue_head: *mut BufferMsgNode,
    /// 0x18: Number of pending messages in the queue.
    pub queue_count: i32,
    // Trailing fields unknown.
}

impl BufferObject {
    /// Rust port of `BufferObject__ClassifyInputMsg` (0x00541100).
    ///
    /// Pops the head node off `queue_head` and updates `read_offset` so
    /// subsequent reads resolve to the node's payload. When the queue
    /// fully drains, `read_offset` re-syncs to `tail_offset` — effectively
    /// resetting the ring.
    ///
    /// The original is a thiscall returning `u32`=1 always.
    #[inline]
    pub unsafe fn classify_input_msg_raw(this: *mut BufferObject) -> bool {
        unsafe {
            let head = (*this).queue_head;
            if head.is_null() {
                return true;
            }
            let next = (*head).next;
            (*this).queue_head = next;
            (*this).queue_count = (*this).queue_count.wrapping_sub(1);
            if next.is_null() {
                (*this).read_offset = (*this).tail_offset;
            } else {
                (*this).read_offset = (next as u32).wrapping_sub((*this).ring_start as u32);
            }
            true
        }
    }

    /// Rust port of `BufferObject__AllocSlot` (0x004F9330).
    ///
    /// Reserves space at `tail_offset` for a message with a `body_size`-byte
    /// body (`body_size = 4 msg_type + payload_bytes`). Returns a pointer
    /// to the `msg_type` slot for the caller to write `[msg_type; payload]`
    /// contiguously, or null if the ring would overrun the read cursor.
    ///
    /// The slot occupies `(body_size + 11) & ~3` bytes of ring space —
    /// node header (8 bytes: `size`, `next`) + body, padded up to 4-byte
    /// alignment. `size` is stored verbatim as the caller's `body_size`.
    ///
    /// Wraps when the tail would exceed `capacity`, provided the wrap
    /// target (offset 0) plus `slot_bytes` doesn't overrun the read cursor.
    pub unsafe fn alloc_slot_raw(this: *mut BufferObject, body_size: u32) -> *mut u32 {
        unsafe {
            let mut tail = (*this).tail_offset;
            let head = (*this).read_offset;
            let slot_bytes = body_size.wrapping_add(11) & !3;

            if tail < head {
                // Read cursor is ahead of write — must not cross it.
                if head <= tail.wrapping_add(slot_bytes) {
                    return core::ptr::null_mut();
                }
            } else if (*this).capacity <= tail.wrapping_add(slot_bytes) {
                // Past end of ring — try wrapping to 0. Rejected if doing
                // so would still overrun the read cursor.
                if head <= slot_bytes {
                    return core::ptr::null_mut();
                }
                tail = 0;
            }

            let slot = (*this).ring_start.add(tail as usize) as *mut BufferMsgNode;
            (*slot).size = body_size;
            (*slot).next = core::ptr::null_mut();

            let prev_tail = (*this).tail_node;
            if !prev_tail.is_null() {
                (*prev_tail).next = slot;
            }
            (*this).tail_node = slot;
            if (*this).queue_head.is_null() {
                (*this).queue_head = slot;
            }
            (*this).queue_count = (*this).queue_count.wrapping_add(1);
            (*this).tail_offset = tail.wrapping_add(slot_bytes);

            &raw mut (*slot).msg_type
        }
    }
}
