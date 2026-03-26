use crate::engine::ddgame::DDGame;
use crate::game::class_type::ClassType;
use crate::FieldRegistry;

crate::define_addresses! {
    class "CTask" {
        /// CTask vtable - 7 virtual method pointers
        vtable CTASK_VTABLE = 0x0066_9F8C;
        /// CTask constructor - initializes base task fields and children list
        ctor/Stdcall CTASK_CONSTRUCTOR = 0x0056_25A0;
        /// CTask::vtable0 - initialization/unknown
        vmethod CTASK_VT0_INIT = 0x0056_2710;
        /// CTask::Free - destructor/deallocation
        vmethod CTASK_VT1_FREE = 0x0056_2620;
        /// CTask::HandleMessage - message dispatch
        vmethod CTASK_VT2_HANDLE_MESSAGE = 0x0056_2F30;
        /// CTask::vtable3 - unknown
        vmethod CTASK_VT3 = 0x0056_13D0;
        /// CTask::vtable5 - unknown
        vmethod CTASK_VT5 = 0x0056_2FA0;
        /// CTask::vtable6 - unknown
        vmethod CTASK_VT6 = 0x0056_3000;
        /// CTask::ProcessFrame
        vmethod CTASK_VT7_PROCESS_FRAME = 0x0056_3210;
    }

    class "CTaskLand" {
        /// CTaskLand vtable - landscape/terrain task
        vtable CTASK_LAND_VTABLE = 0x0066_4388;
        ctor CTASK_LAND_CTOR = 0x0050_5440;
    }

    class "CTaskDirt" {
        /// CTaskDirt vtable - dirt/particle system (1 per game)
        vtable CTASK_DIRT_VTABLE = 0x0066_9D74;
        ctor CTASK_DIRT_CTOR = 0x0054_EDC0;
    }

    class "CTaskSpriteAnim" {
        /// CTaskSpriteAnim vtable - sprite animation manager (1 per game)
        vtable CTASK_SPRITE_ANIM_VTABLE = 0x0066_9D00;
        ctor CTASK_SPRITE_ANIM_CTOR = 0x0054_66C0;
    }

    class "CTaskCPU" {
        /// CTaskCPU vtable - AI/CPU bot controller
        vtable CTASK_CPU_VTABLE = 0x0066_9D54;
        ctor CTASK_CPU_CTOR = 0x0054_85D0;
    }

    class "CTaskSeaBubble" {
        /// CTaskSeaBubble vtable - water bubble particle
        vtable CTASK_SEA_BUBBLE_VTABLE = 0x0066_9E88;
        ctor CTASK_SEABUBBLE_CTOR = 0x0055_4FE0;
    }

    // Entity constructors without known vtables
    class "CTaskAirstrike" {
        ctor CTASK_AIRSTRIKE_CTOR = 0x0055_53C0;
    }
    class "CTaskArrow" {
        ctor CTASK_ARROW_CTOR = 0x004F_E130;
    }
    class "CTaskCanister" {
        ctor CTASK_CANISTER_CTOR = 0x0050_1A80;
    }
    class "CTaskCross" {
        ctor CTASK_CROSS_CTOR = 0x0050_45C0;
    }
    class "CTaskFireball" {
        ctor CTASK_FIREBALL_CTOR = 0x0055_0890;
    }
    class "CTaskFlame" {
        ctor CTASK_FLAME_CTOR = 0x0054_F0F0;
    }
    class "CTaskGas" {
        ctor CTASK_GAS_CTOR = 0x0055_4750;
    }
    class "CTaskOldWorm" {
        ctor CTASK_OLDWORM_CTOR = 0x0051_FEB0;
    }
    class "CTaskScoreBubble" {
        ctor CTASK_SCOREBUBBLE_CTOR = 0x0055_4CA0;
    }
    class "CTaskSmoke" {
        ctor CTASK_SMOKE_CTOR = 0x0055_51D0;
    }
}

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
#[derive(FieldRegistry)]
#[repr(C)]
pub struct CTask<V: 'static = *const core::ffi::c_void> {
    /// 0x00: Pointer to virtual method table
    pub vtable: V,
    /// 0x04: Parent task in the hierarchy
    pub parent: *mut u8,
    /// 0x08: Children array capacity — starts at 0x10, doubles via realloc when full.
    pub children_capacity: u32,
    /// 0x0C: Set to 1 by `FUN_004fdce0` when a child slot is nulled (dirty flag).
    /// Zero at construction. Not decremented; purely a "child was removed" marker.
    pub children_dirty: u32,
    /// 0x10: Insert watermark — incremented on every child insertion, never decremented
    /// on removal. Dead children leave null slots; `children_data[0..children_watermark]`
    /// is a sparse array. This grows without bound within a session (e.g., sea bubbles
    /// continuously spawn/die, each consuming a new slot and doubling capacity as needed).
    pub children_watermark: u32,
    /// 0x14: Pointer to children data array (sparse, allocated 0x60 bytes initially,
    /// reallocated to `children_capacity * 8 + 0x20` bytes on overflow).
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
        Self {
            buckets: ptr as *const *mut SharedDataNode,
        }
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

    /// Look up an entity by key pair. Returns the entity pointer, or null.
    ///
    /// Pure Rust equivalent of `FUN_004FDF90` (SharedData__Lookup).
    /// fastcall(ECX=key_esi, EDX=key_edi, stack=task) in the original.
    ///
    /// # Safety
    /// The table and all linked nodes must be valid.
    pub unsafe fn lookup(&self, key_esi: u32, key_edi: u32) -> *mut u8 {
        let bucket = Self::bucket_for(key_esi, key_edi) as usize;
        let mut node = *self.buckets.add(bucket);
        while !node.is_null() {
            if (*node).key_edi == key_edi && (*node).key_esi == key_esi {
                return (*node).entity;
            }
            node = (*node).next;
        }
        core::ptr::null_mut()
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

/// Breadth-first iterator over the CTask tree.
///
/// Visits every node reachable from `root` by following `children_data`
/// arrays. Null slots in the sparse children array are skipped automatically.
///
/// Yields raw `*const CTask` pointers. The caller is responsible for casting
/// to the correct derived type (e.g., by checking the vtable pointer at `[0]`).
///
/// # Example
/// ```ignore
/// let iter = unsafe { CTaskBfsIter::new(root_ptr) };
/// for task in iter {
///     if unsafe { *(task as *const u32) } == rb(va::CTASK_MISSILE_VTABLE) {
///         let m = unsafe { &*(task as *const CTaskMissile) };
///         // ...
///     }
/// }
/// ```
pub struct CTaskBfsIter {
    queue: std::collections::VecDeque<*const CTask>,
}

impl CTaskBfsIter {
    /// Create a new BFS iterator rooted at `root`.
    ///
    /// # Safety
    /// `root` must be a valid, aligned `*const CTask`. All reachable
    /// `children_data` entries must be either null or valid `*const CTask`.
    pub unsafe fn new(root: *const CTask) -> Self {
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(root);
        Self { queue }
    }
}

impl Iterator for CTaskBfsIter {
    type Item = *const CTask;

    fn next(&mut self) -> Option<Self::Item> {
        // SAFETY: caller of CTaskBfsIter::new() guarantees node validity.
        unsafe {
            let node = self.queue.pop_front()?;
            let watermark = (*node).children_watermark as usize;
            let data = (*node).children_data as *const u32;
            if !data.is_null() {
                for i in 0..watermark {
                    let child_ptr = *data.add(i);
                    if child_ptr != 0 {
                        self.queue.push_back(child_ptr as *const CTask);
                    }
                }
            }
            Some(node)
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
