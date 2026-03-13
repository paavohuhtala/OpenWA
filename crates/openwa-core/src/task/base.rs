use crate::class_type::ClassType;
use crate::ddgame::DDGame;

/// Base task class in WA's entity hierarchy.
///
/// All game objects inherit from CTask. Tasks form a tree via parent/children
/// pointers and communicate through the TaskMessage system.
///
/// Source: wkJellyWorm CTask.h, Ghidra decompilation of 0x5625A0 + 0x562520
///
/// Vtable at 0x669F8C (8 methods):
///   0x00: 0x562710 vtable0 (init?)
///   0x04: 0x562620 Free
///   0x08: 0x562F30 HandleMessage
///   0x0C: 0x5613D0 unknown
///   0x10: 0x5613D0 unknown (same as 0x0C)
///   0x14: 0x562FA0 unknown
///   0x18: 0x563000 unknown
///   0x1C: 0x563210 ProcessFrame
#[repr(C)]
pub struct CTask {
    /// 0x00: Pointer to virtual method table
    pub vtable: *mut u8,
    /// 0x04: Parent task in the hierarchy
    pub parent: *mut u8,
    /// 0x08: Children list max capacity (set to 0x10 in constructor)
    pub children_max_size: u32,
    /// 0x0C: Children list unknown field (set to 0 in constructor)
    pub children_unk: u32,
    /// 0x10: Children list current size
    pub children_size: u32,
    /// 0x14: Pointer to children data array (allocated 0x60 bytes in constructor)
    pub children_data: *mut u8,
    /// 0x18: Children hash list pointer (set to 0 in constructor)
    pub children_hash: *mut u8,
    /// 0x1C: Unknown (set to 0 by parent-linking helper FUN_00562520)
    pub _unknown_1c: u32,
    /// 0x20: Task classification type (set to ClassType::Task by FUN_00562520,
    /// overridden by derived constructors)
    pub class_type: ClassType,
    /// 0x24: Shared data buffer pointer (inherited from parent, or allocated
    /// 0x420 bytes for root tasks)
    pub shared_data: *mut u8,
    /// 0x28: 1 if this task owns shared_data (root), 0 if inherited from parent
    pub owns_shared_data: u32,
    /// 0x2C: DDGame pointer (3rd param to CTask::Constructor, stored at this+0x2C)
    pub ddgame: *mut DDGame,
}

const _: () = assert!(core::mem::size_of::<CTask>() == 0x30);

// ---------------------------------------------------------------------------
// Shared-data entity registry
// ---------------------------------------------------------------------------

/// A 0x30-byte node in CTask's shared-data entity hash table.
///
/// Inserted by `SharedData__Insert` (0x5406A0, called from task constructors).
/// All game task types (CTaskWorm, CTaskLand, projectiles, …) share the same
/// 256-bucket table at `CTask.shared_data`. Use the vtable pointer at
/// `entity[0]` to identify the object type.
///
/// Hash function (from Ghidra decompilation of `SharedData__Insert`):
/// ```text
/// bucket = (key_esi * 0x11 + key_edi) & 0x800000ff;
/// if (int)bucket < 0 { bucket = bucket.wrapping_sub(1) | 0xffffff00; bucket += 1; }
/// ```
/// In practice (small positive key values), this reduces to:
/// `bucket = (key_esi * 0x11 + key_edi) & 0xff`
///
/// Runtime observation: for `CTaskWorm`, `key_esi` encodes a compound worm
/// identity (e.g. `0x11` = team 1, worm 1) and `key_edi` is a small integer.
/// Companion remove function: `SharedData__Remove` (0x540700).
#[repr(C)]
pub struct SharedDataNode {
    /// +0x00: Next node in this bucket's linked list (null = end).
    pub next: *mut SharedDataNode,
    /// +0x04: EDI register value at registration time.
    pub key_edi: u32,
    /// +0x08: ESI register value at registration time.
    pub key_esi: u32,
    /// +0x0C: Registered entity pointer (first DWORD = vtable).
    pub entity: *mut u8,
    /// +0x10..0x2F: Unused allocation padding.
    pub _padding: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<SharedDataNode>() == 0x30);

/// View of the 256-bucket entity hash table at `CTask.shared_data`.
///
/// Root tasks own 0x420 bytes of shared data:
/// - `0x000..0x3FF`: 256 × `*mut SharedDataNode` bucket heads
/// - `0x400..0x41F`: Other root-task data (layout unknown)
///
/// All tasks in the same game tree inherit the same `shared_data` pointer, so
/// any task can be used to access the full table. Use [`SharedDataTable::iter`]
/// to walk all registered entities and filter by vtable address.
///
/// Registered by `SharedData__Insert` (0x5406A0); removed by
/// `SharedData__Remove` (0x540700).
pub struct SharedDataTable {
    buckets: *const *mut SharedDataNode,
}

impl SharedDataTable {
    /// Construct from a raw `CTask.shared_data` pointer.
    ///
    /// # Safety
    /// `ptr` must point to a valid shared-data region of at least 256 × 4 = 1024 bytes.
    pub unsafe fn from_ptr(ptr: *mut u8) -> Self {
        Self { buckets: ptr as *const *mut SharedDataNode }
    }

    /// Construct from a `CTask` pointer (reads `task.shared_data`).
    ///
    /// # Safety
    /// `task` must be a valid, aligned `CTask` pointer.
    pub unsafe fn from_task(task: *const CTask) -> Self {
        Self::from_ptr((*task).shared_data)
    }

    /// Compute the bucket index for a (key_esi, key_edi) pair.
    ///
    /// Exact transcription of the hash in `FUN_005406a0`.
    pub fn bucket_for(key_esi: u32, key_edi: u32) -> u32 {
        let mut h = key_esi.wrapping_mul(0x11).wrapping_add(key_edi) & 0x800000ff;
        if (h as i32) < 0 {
            h = h.wrapping_sub(1) | 0xffffff00;
            h = h.wrapping_add(1);
        }
        h
    }

    /// Iterate all nodes across all 256 buckets.
    ///
    /// # Safety
    /// The table and all linked nodes must be valid and not concurrently modified.
    pub unsafe fn iter(&self) -> SharedDataIter {
        SharedDataIter {
            buckets: self.buckets,
            bucket: 0,
            node: core::ptr::null_mut(),
        }
    }
}

/// Iterator over all [`SharedDataNode`]s in a [`SharedDataTable`].
///
/// Created by [`SharedDataTable::iter`]. Walks all 256 buckets in order,
/// following `next` pointers within each bucket.
pub struct SharedDataIter {
    buckets: *const *mut SharedDataNode,
    bucket: usize,
    node: *mut SharedDataNode,
}

impl Iterator for SharedDataIter {
    type Item = *mut SharedDataNode;

    fn next(&mut self) -> Option<Self::Item> {
        // SAFETY: caller of SharedDataTable::iter() guarantees table validity.
        unsafe {
            loop {
                if !self.node.is_null() {
                    let current = self.node;
                    self.node = (*self.node).next;
                    return Some(current);
                }
                if self.bucket >= 256 {
                    return None;
                }
                self.node = *self.buckets.add(self.bucket);
                self.bucket += 1;
            }
        }
    }
}
