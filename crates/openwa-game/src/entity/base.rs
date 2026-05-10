use crate::FieldRegistry;
use crate::engine::world::GameWorld;
use crate::entity::WorldRootEntity;
use crate::game::EntityMessage;
use crate::game::class_type::ClassType;
use crate::game::message::EntityMessageData;

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
    /// 0x0C: Set to 1 by `BaseEntity__UnlinkChild` when a child slot is nulled (dirty flag).
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
    /// 0x1C: Unknown (set to 0 by parent-linking helper BaseEntity__LinkToParent)
    pub _unknown_1c: u32,
    /// 0x20: Entity classification type (set to ClassType::Entity by BaseEntity__LinkToParent,
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

    fn world_root(&self) -> *mut WorldRootEntity {
        unsafe {
            let base = self.entity();
            WorldRootEntity::from_entity(base)
        }
    }

    fn game_version(&self) -> i32 {
        unsafe {
            let world = self.world();
            if world.is_null() {
                return 0;
            }
            (*(*world).game_info).game_version
        }
    }

    /// Broadcast a message to all children — pure Rust port of BaseEntity::HandleMessage (0x562F30).
    ///
    /// Iterates the sparse children array (`children_data[0..children_watermark]`),
    /// skips null entries, and calls each child's `HandleMessage` (vtable slot 2).
    /// This is how messages propagate down the entity tree.
    ///
    /// # Safety
    /// All non-null children must be valid BaseEntity pointers with valid vtables.
    unsafe fn handle_message_raw(
        this: *mut Self,
        sender: *mut BaseEntity,
        msg_type: EntityMessage,
        size: u32,
        data: *const u8,
    ) {
        unsafe {
            let entity_ptr = this as *mut BaseEntity;

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

    unsafe fn broadcast_via_world_root<TMessage: EntityMessageData>(&mut self, message: TMessage) {
        unsafe {
            let world_root = self.world_root();
            if world_root.is_null() {
                return;
            }
            WorldRootEntity::broadcast_raw(world_root, self.as_entity_ptr_mut(), message);
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
// Tree iteration
// ---------------------------------------------------------------------------

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
