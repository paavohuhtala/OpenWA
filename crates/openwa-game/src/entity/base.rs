use crate::FieldRegistry;
use crate::engine::world::GameWorld;
use crate::game::EntityMessage;
use crate::game::class_type::ClassType;

crate::define_addresses! {
    class "BaseEntity" {
        /// BaseEntity constructor - initializes base entity fields and children list
        ctor/Stdcall BASE_ENTITY_CONSTRUCTOR = 0x005625A0;
        /// BaseEntity::vtable0 - initialization/unknown
        vmethod BASE_ENTITY_VT0_INIT = 0x00562710;
        /// BaseEntity::Free - destructor/deallocation
        vmethod BASE_ENTITY_VT1_FREE = 0x00562620;
        /// BaseEntity::HandleMessage - message dispatch
        vmethod BASE_ENTITY_VT2_HANDLE_MESSAGE = 0x00562F30;
        /// BaseEntity::vtable3 - unknown
        vmethod BASE_ENTITY_VT3 = 0x005613D0;
        /// BaseEntity::vtable5 - unknown
        vmethod BASE_ENTITY_VT5 = 0x00562FA0;
        /// BaseEntity::vtable6 - unknown
        vmethod BASE_ENTITY_VT6 = 0x00563000;
        /// BaseEntity::ProcessFrame
        vmethod BASE_ENTITY_VT7_PROCESS_FRAME = 0x00563210;
    }

    class "LandEntity" {
        /// LandEntity vtable - landscape/terrain entity
        vtable LAND_ENTITY_VTABLE = 0x00664388;
        ctor LAND_ENTITY_CTOR = 0x00505440;
    }

    class "DirtEntity" {
        /// DirtEntity vtable - dirt/particle system (1 per game)
        vtable DIRT_ENTITY_VTABLE = 0x00669D74;
        ctor DIRT_ENTITY_CTOR = 0x0054EDC0;
    }

    class "SpriteAnimEntity" {
        /// SpriteAnimEntity vtable - sprite animation manager (1 per game)
        vtable SPRITE_ANIM_ENTITY_VTABLE = 0x00669D00;
        ctor SPRITE_ANIM_ENTITY_CTOR = 0x005466C0;
    }

    class "CPUEntity" {
        /// CPUEntity vtable - AI/CPU bot controller
        vtable CPU_ENTITY_VTABLE = 0x00669D54;
        ctor CPU_ENTITY_CTOR = 0x005485D0;
    }

    class "SeaBubbleEntity" {
        /// SeaBubbleEntity vtable - water bubble particle
        vtable SEA_BUBBLE_ENTITY_VTABLE = 0x00669E88;
        ctor SEABUBBLE_ENTITY_CTOR = 0x00554FE0;
    }

    // Entity constructors without known vtables
    class "AirstrikeEntity" {
        ctor AIRSTRIKE_ENTITY_CTOR = 0x005553C0;
    }
    class "ArrowEntity" {
        ctor ARROW_ENTITY_CTOR = 0x004FE130;
    }
    class "CanisterEntity" {
        ctor CANISTER_ENTITY_CTOR = 0x00501A80;
    }
    class "CrossEntity" {
        ctor CROSS_ENTITY_CTOR = 0x005045C0;
    }
    class "FireballEntity" {
        ctor FIREBALL_ENTITY_CTOR = 0x00550890;
    }
    class "FlameEntity" {
        ctor FLAME_ENTITY_CTOR = 0x0054F0F0;
    }
    class "GasEntity" {
        ctor GAS_ENTITY_CTOR = 0x00554750;
    }
    class "OldWarmEntity" {
        ctor OLDWORM_ENTITY_CTOR = 0x0051FEB0;
    }
    class "ScoreBubbleEntity" {
        ctor SCOREBUBBLE_ENTITY_CTOR = 0x00554CA0;
    }
    class "SmokeEntity" {
        ctor SMOKE_ENTITY_CTOR = 0x005551D0;
    }
}

/// BaseEntity base vtable — 7 slots shared by all entity types.
///
/// Every BaseEntity subclass vtable starts with these 7 slots. Subclasses override
/// individual slots and extend with additional class-specific methods.
/// WorldEntity adds slots 7+ beyond this base.
///
/// Source: wkJellyWorm/src/entities/CTask.h (vtNum = 7), confirmed via Ghidra.
///
/// ```text
/// Slot  Offset  Name                 Params (thiscall, ECX=this)
/// ----  ------  -------------------  ----------------------------
///  0    0x00    WriteReplayState     stream: *mut u8
///  1    0x04    Free                 flags: u8 → *mut BaseEntity
///  2    0x08    HandleMessage        sender, msg_type, size, data
///  3    0x0C    (unknown, stub)      3 params, returns 0
///  4    0x10    (unknown, stub)      3 params, returns 0 (same fn as slot 3)
///  5    0x14    ProcessChildren      flags: u32
///  6    0x18    ProcessFrame         (no params)
/// ```
#[openwa_game::vtable(size = 7, va = 0x00669F8C, class = "BaseEntity")]
pub struct BaseEntityVtable {
    /// WriteReplayState — serializes entity state to replay stream.
    #[slot(0)]
    pub write_replay_state: fn(this: *mut BaseEntity, stream: *mut u8),
    /// Free — scalar deleting destructor. Calls destructor, then `_free` if flags & 1.
    #[slot(1)]
    pub free: fn(this: *mut BaseEntity, flags: u8) -> *mut BaseEntity,
    /// HandleMessage — broadcasts message to all children (base implementation).
    #[slot(2)]
    pub handle_message: fn(
        this: *mut BaseEntity,
        sender: *mut BaseEntity,
        msg_type: EntityMessage,
        size: u32,
        data: *const u8,
    ),
    /// ProcessChildren — iterates children with flags. Base at 0x562FA0.
    #[slot(5)]
    pub process_children: fn(this: *mut BaseEntity, flags: u32),
    /// ProcessFrame — per-frame update. Base iterates children. At 0x563000.
    #[slot(6)]
    pub process_frame: fn(this: *const BaseEntity),
}

bind_BaseEntityVtable!(BaseEntity, vtable);

/// Base entity class in WA's entity hierarchy.
///
/// All game objects inherit from BaseEntity. Entities form a tree via parent/children
/// pointers and communicate through the EntityMessage system.
///
/// Source: wkJellyWorm CTask.h, Ghidra decompilation of 0x5625A0 + 0x562520
///   0x1C: 0x563210 ProcessFrame
#[derive(FieldRegistry)]
#[repr(C)]
pub struct BaseEntity<V: Vtable = *const BaseEntityVtable> {
    /// 0x00: Pointer to virtual method table
    pub vtable: V,
    /// 0x04: Parent entity in the hierarchy
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
    /// Non-null entries are valid BaseEntity pointers (any subclass).
    pub children_data: *mut *mut BaseEntity,
    /// 0x18: Children hash list pointer (set to 0 in constructor)
    pub children_hash: *mut u8,
    /// 0x1C: Unknown (set to 0 by parent-linking helper FUN_00562520)
    pub _unknown_1c: u32,
    /// 0x20: Entity classification type (set to ClassType::Entity by FUN_00562520,
    /// overridden by derived constructors)
    pub class_type: ClassType,
    /// 0x24: Shared data buffer pointer (inherited from parent, or allocated
    /// 0x420 bytes for root entities)
    pub shared_data: *mut u8,
    /// 0x28: 1 if this entity owns shared_data (root), 0 if inherited from parent
    pub owns_shared_data: u32,
    /// 0x2C: GameWorld pointer (3rd param to BaseEntity::Constructor, stored at this+0x2C)
    pub world: *mut GameWorld,
}

const _: () = assert!(core::mem::size_of::<BaseEntity>() == 0x30);

/// Marker trait for types that can be used as vtable pointers in `BaseEntity<V>`.
///
/// Implemented automatically by the `#[vtable]` proc macro for `*const MyVtable`.
/// Also implemented for `*const c_void` (the default/untyped case).
///
/// # Safety
/// Implementors must be pointer-sized types pointing to valid vtable data.
pub unsafe trait Vtable: 'static {}

// Default vtable type (untyped)
unsafe impl Vtable for *const core::ffi::c_void {}

/// Trait for all entity types in the BaseEntity hierarchy.
///
/// Provides safe access to the underlying BaseEntity fields regardless of
/// inheritance depth. Avoids repetitive `.base.base` chains.
///
/// # Safety
/// Implementors must be `#[repr(C)]` structs where a `BaseEntity` (with any
/// vtable type parameter) is at offset 0.
pub unsafe trait Entity {
    /// Get a shared reference to the underlying BaseEntity (default vtable type).
    fn entity(&self) -> &BaseEntity {
        unsafe { &*(self as *const Self as *const BaseEntity) }
    }

    /// Get a mutable reference to the underlying BaseEntity.
    fn entity_mut(&mut self) -> &mut BaseEntity {
        unsafe { &mut *(self as *mut Self as *mut BaseEntity) }
    }

    /// Get a raw const pointer to the BaseEntity base.
    fn as_entity_ptr(&self) -> *const BaseEntity {
        self as *const Self as *const BaseEntity
    }

    /// Get a raw mutable pointer to the BaseEntity base.
    fn as_entity_ptr_mut(&mut self) -> *mut BaseEntity {
        self as *mut Self as *mut BaseEntity
    }

    /// Get the GameWorld pointer from the BaseEntity base.
    fn world(&self) -> *mut GameWorld {
        self.entity().world
    }

    /// Broadcast a message to all children — pure Rust port of BaseEntity::HandleMessage (0x562F30).
    ///
    /// Iterates the sparse children array (`children_data[0..children_watermark]`),
    /// skips null entries, and calls each child's `HandleMessage` (vtable slot 2).
    /// This is how messages propagate down the entity tree.
    ///
    /// # Safety
    /// All non-null children must be valid BaseEntity pointers with valid vtables.
    unsafe fn broadcast_message(
        &mut self,
        sender: *mut BaseEntity,
        msg_type: EntityMessage,
        size: u32,
        data: *const u8,
    ) {
        unsafe {
            let entity_ptr = self.as_entity_ptr_mut();

            // Scan for non-null children and dispatch HandleMessage.
            // Mirrors WA's BaseEntity::HandleMessage at 0x562F30 exactly:
            // scan → dispatch → re-read watermark → scan next → ...
            //
            // IMPORTANT: read_volatile is required for watermark and children_data
            // because child handlers may modify this entity's children array
            // (add/remove children, realloc the array). LLVM would otherwise cache
            // these reads across the virtual dispatch call.
            let mut i: usize = 0;
            loop {
                // Scan for next non-null child
                let child = loop {
                    let watermark = core::ptr::read_volatile(core::ptr::addr_of!(
                        (*entity_ptr).children_watermark
                    )) as usize;
                    if i >= watermark {
                        return;
                    }
                    let children =
                        core::ptr::read_volatile(core::ptr::addr_of!((*entity_ptr).children_data));
                    let c = *children.add(i);
                    i += 1;
                    if !c.is_null() {
                        break c;
                    }
                };

                BaseEntity::handle_message_raw(child, sender, msg_type, size, data);
            }
        }
    }
}

// Blanket impl for any BaseEntity<V>
unsafe impl<V: Vtable> Entity for BaseEntity<V> {}

// ---------------------------------------------------------------------------
// Raw-pointer associated functions — no &self/&mut self, no noalias UB.
//
// Use these instead of Entity trait methods when operating on WA-owned objects
// through raw pointers. Any type whose first field is BaseEntity (at offset 0)
// can be cast to *mut BaseEntity and used with these functions.
// ---------------------------------------------------------------------------

impl BaseEntity {
    /// Read the dword at object offset +0x30 — the collision system's
    /// "contact face" scratch slot.
    ///
    /// Right before dispatching slot 8 (`OnContact`) on a WorldEntity, the
    /// physics/collision dispatcher writes the face index of the contact
    /// (0..31) into this slot on the *contacted* object. The callee reads the
    /// low 5 bits and uses `1 << face_idx` to test against per-object face
    /// masks.
    ///
    /// This slot overlaps `WorldEntity::subclass_data[0..4]`, which several
    /// subclasses repurpose as durable storage (worm weapon-fire type,
    /// world_root/team secondary vtable pointer, cloud parallax depth). Outside
    /// of OnContact dispatch, the value here is whatever the subclass wrote,
    /// not a face index — only read this during contact dispatch.
    #[inline(always)]
    pub unsafe fn contact_face_slot_raw(this: *const BaseEntity) -> u32 {
        unsafe { *((this as *const u8).add(0x30) as *const u32) }
    }

    /// Broadcast a message to all children — raw-pointer version.
    ///
    /// Pure Rust port of BaseEntity::HandleMessage (0x562F30).
    /// Identical to `Entity::broadcast_message` but takes `*mut BaseEntity` instead
    /// of `&mut self`, avoiding noalias UB.
    pub unsafe fn broadcast_message_raw(
        entity_ptr: *mut BaseEntity,
        sender: *mut BaseEntity,
        msg_type: EntityMessage,
        size: u32,
        data: *const u8,
    ) {
        unsafe {
            let mut i: usize = 0;
            loop {
                let child = loop {
                    let watermark = core::ptr::read_volatile(core::ptr::addr_of!(
                        (*entity_ptr).children_watermark
                    )) as usize;
                    if i >= watermark {
                        return;
                    }
                    let children =
                        core::ptr::read_volatile(core::ptr::addr_of!((*entity_ptr).children_data));
                    let c = *children.add(i);
                    i += 1;
                    if !c.is_null() {
                        break c;
                    }
                };

                let vt = &*((*child).vtable as *const BaseEntityVtable);
                (vt.handle_message)(child, sender, msg_type, size, data);
            }
        }
    }

    /// Typed wrapper around [`BaseEntity::broadcast_message_raw`] — serialises a
    /// `EntityMessageData` payload and uses its `MESSAGE_TYPE` for dispatch.
    pub unsafe fn broadcast_typed_message_raw<TMessage: crate::game::message::EntityMessageData>(
        entity_ptr: *mut BaseEntity,
        sender: *mut BaseEntity,
        message: TMessage,
    ) {
        let buf = bytemuck::bytes_of(&message);
        let size = buf.len() as u32;
        unsafe {
            let data = if size > 0 {
                buf.as_ptr()
            } else {
                core::ptr::null()
            };
            Self::broadcast_message_raw(entity_ptr, sender, TMessage::MESSAGE_TYPE, size, data);
        }
    }
}

// ---------------------------------------------------------------------------
// Shared-data entity registry
// ---------------------------------------------------------------------------

/// A 0x30-byte node in BaseEntity's shared-data entity hash table.
///
/// Inserted by `SharedData__Insert` (0x5406A0, called from entity constructors).
/// All game entity types (WormEntity, LandEntity, projectiles, …) share the same
/// 256-bucket table at `BaseEntity.shared_data`. Use the vtable pointer at
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
/// Runtime observation: for `WormEntity`, `key_esi` encodes a compound worm
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

/// View of the 256-bucket entity hash table at `BaseEntity.shared_data`.
///
/// Root entities own 0x420 bytes of shared data:
/// - `0x000..0x3FF`: 256 × `*mut SharedDataNode` bucket heads
/// - `0x400..0x41F`: Other root-entity data (layout unknown)
///
/// All entities in the same game tree inherit the same `shared_data` pointer, so
/// any entity can be used to access the full table. Use [`SharedDataTable::iter`]
/// to walk all registered entities and filter by vtable address.
///
/// Registered by `SharedData__Insert` (0x5406A0); removed by
/// `SharedData__Remove` (0x540700).
pub struct SharedDataTable {
    buckets: *const *mut SharedDataNode,
}

impl SharedDataTable {
    /// Construct from a raw `BaseEntity.shared_data` pointer.
    ///
    /// # Safety
    /// `ptr` must point to a valid shared-data region of at least 256 × 4 = 1024 bytes.
    pub unsafe fn from_ptr(ptr: *mut u8) -> Self {
        Self {
            buckets: ptr as *const *mut SharedDataNode,
        }
    }

    /// Construct from a `BaseEntity` pointer (reads `entity.shared_data`).
    ///
    /// # Safety
    /// `entity` must be a valid, aligned `BaseEntity` pointer.
    pub unsafe fn from_task(entity: *const BaseEntity) -> Self {
        unsafe { Self::from_ptr((*entity).shared_data) }
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
    /// fastcall(ECX=key_esi, EDX=key_edi, stack=entity) in the original.
    ///
    /// # Safety
    /// The table and all linked nodes must be valid.
    pub unsafe fn lookup(&self, key_esi: u32, key_edi: u32) -> *mut u8 {
        unsafe {
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

/// Breadth-first iterator over the BaseEntity tree.
///
/// Visits every node reachable from `root` by following `children_data`
/// arrays. Null slots in the sparse children array are skipped automatically.
///
/// Yields raw `*const BaseEntity` pointers. The caller is responsible for casting
/// to the correct derived type (e.g., by checking the vtable pointer at `[0]`).
///
/// # Example
/// ```ignore
/// let iter = unsafe { BaseEntityBfsIter::new(root_ptr) };
/// for entity in iter {
///     if unsafe { *(entity as *const u32) } == rb(va::MISSILE_ENTITY_VTABLE) {
///         let m = unsafe { &*(entity as *const MissileEntity) };
///         // ...
///     }
/// }
/// ```
pub struct BaseEntityBfsIter {
    queue: std::collections::VecDeque<*const BaseEntity>,
}

impl BaseEntityBfsIter {
    /// Create a new BFS iterator rooted at `root`.
    ///
    /// # Safety
    /// `root` must be a valid, aligned `*const BaseEntity`. All reachable
    /// `children_data` entries must be either null or valid `*const BaseEntity`.
    pub unsafe fn new(root: *const BaseEntity) -> Self {
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(root);
        Self { queue }
    }
}

impl Iterator for BaseEntityBfsIter {
    type Item = *const BaseEntity;

    fn next(&mut self) -> Option<Self::Item> {
        // SAFETY: caller of BaseEntityBfsIter::new() guarantees node validity.
        unsafe {
            let node = self.queue.pop_front()?;
            let watermark = (*node).children_watermark as usize;
            let data = (*node).children_data as *const u32;
            if !data.is_null() {
                for i in 0..watermark {
                    let child_ptr = *data.add(i);
                    if child_ptr != 0 {
                        self.queue.push_back(child_ptr as *const BaseEntity);
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
