//! Partial layout of `BufferObject` — the ring/message buffer used by
//! `DDGameWrapper` for input dispatch, serialization, and the state buffer.
//!
//! Full layout is unknown; only the fields touched by `classify_input_msg`
//! (0x00541100) and `input_queue_drain` are typed here. The rest is left
//! as opaque padding so the struct is not yet sized.

use crate::FieldRegistry;

/// A single message node in `BufferObject::queue_head`.
///
/// Layout at the `queue_head` pointer:
/// - `+0x00` size in bytes (header + payload; payload = `size - 4`)
/// - `+0x04` next node (null = end of queue)
/// - `+0x08` message type discriminator
/// - `+0x0C` payload (`size - 4` bytes)
#[repr(C)]
pub struct BufferMsgNode {
    pub size: u32,
    pub next: *mut BufferMsgNode,
    pub msg_type: u32,
    // payload follows inline
}

/// Partial view of `BufferObject` — only the queue/read-cursor fields used
/// by `classify_input_msg` are typed.
///
/// Used via raw pointer; the full size is not known, so this struct is
/// `#[repr(C)]` with typed prefix + dummy sentinel to enforce non-use of
/// `size_of`.
#[derive(FieldRegistry)]
#[repr(C)]
pub struct BufferObject {
    /// 0x00: Start pointer of the backing ring (raw bytes).
    pub ring_start: *mut u8,
    /// 0x04: Unknown.
    pub _field_04: u32,
    /// 0x08: Default/reset offset written into `read_offset` when the queue
    /// is drained (`queue_head` becomes null).
    pub reset_offset: u32,
    /// 0x0C: Current read offset relative to `ring_start`.
    pub read_offset: u32,
    /// 0x10: Unknown.
    pub _field_10: u32,
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
    /// subsequent reads resolve to the node's payload. The original is a
    /// thiscall returning `u32` always `1` (despite the call site's earlier
    /// FFI bridge typing it as `u64` — EDX was never written, so the high
    /// half was unspecified). Returns `true` for call-site compatibility.
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
                (*this).read_offset = (*this).reset_offset;
            } else {
                (*this).read_offset = (next as u32).wrapping_sub((*this).ring_start as u32);
            }
            true
        }
    }
}
