use crate::class_type::ClassType;
use crate::ddgame::DDGame;
use crate::fixed::Fixed;

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

/// Game task - extends CTask with physics and gameplay data.
///
/// PARTIAL: Most fields between 0x30-0x83 and 0x98-0xE7 are unknown.
/// Only position and velocity fields have been verified.
///
/// Source: wkJellyWorm CGameTask.h
///
/// Additional vtable (12 methods at offsets 0x1C-0x48 in vtable)
#[repr(C)]
pub struct CGameTask {
    /// 0x00-0x2F: Base CTask fields
    pub base: CTask,
    /// 0x30-0x83: Unknown gameplay fields (84 bytes)
    pub _unknown_30: [u8; 0x54],
    /// 0x84: X position in fixed-point
    pub pos_x: Fixed,
    /// 0x88: Y position in fixed-point
    pub pos_y: Fixed,
    /// 0x8C-0x8F: Unknown (4 bytes between pos and speed)
    pub _unknown_8c: [u8; 4],
    /// 0x90: X velocity in fixed-point
    pub speed_x: Fixed,
    /// 0x94: Y velocity in fixed-point
    pub speed_y: Fixed,
    /// 0x98-0xE7: Unknown gameplay fields
    pub _unknown_98: [u8; 0x50],
    /// 0xE8: Embedded sound emitter sub-object (MSVC multiple inheritance).
    pub sound_emitter: SoundEmitter,
}

const _: () = assert!(core::mem::size_of::<CGameTask>() == 0xFC);

/// Sound emitter sub-object embedded in CGameTask via MSVC multiple inheritance.
///
/// Provides spatial audio support. The `this` pointer for its vtable methods
/// points to the start of this sub-object (CGameTask+0xE8), not the CGameTask.
#[repr(C)]
pub struct SoundEmitter {
    /// +0x00: Vtable pointer
    pub vtable: *const SoundEmitterVTable,
    /// +0x04-0x0B: Unknown fields
    pub _unknown_04: [u8; 8],
    /// +0x0C: Number of active local sounds
    pub local_sound_count: i32,
    /// +0x10: Back-pointer to containing CGameTask
    pub owner: *mut CGameTask,
}

const _: () = assert!(core::mem::size_of::<SoundEmitter>() == 0x14);

/// Vtable for the SoundEmitter sub-object (0x669CF8, 12 slots).
///
/// Slots [0]-[4] are the sound emitter's own interface.
/// Slots [5]-[11] are inherited CTask base methods.
#[repr(C)]
pub struct SoundEmitterVTable {
    /// [0] 0x546680: GetPosition(this, out_x, out_y) — reads pos_x/pos_y via owner
    pub get_position: unsafe extern "thiscall" fn(*const SoundEmitter, *mut u32, *mut u32),
    /// [1] 0x5466A0: GetPosition2(this, out_x, out_y) — reads CGameTask+0x38/0x3C
    pub get_position2: unsafe extern "thiscall" fn(*const SoundEmitter, *mut u32, *mut u32),
    /// [2] 0x4260E0: Unknown
    pub _unknown_2: *const (),
    /// [3] 0x546990: Destructor(this, flags)
    pub destructor: unsafe extern "thiscall" fn(*mut SoundEmitter, u32) -> *mut SoundEmitter,
    /// [4] 0x546760: HandleMessage — sound queue manager
    pub handle_message: unsafe extern "thiscall" fn(*mut SoundEmitter, u32, u32, u32, u32),
    /// [5]-[11]: Inherited CTask base methods
    pub _base_methods: [*const (); 7],
}

/// Virtual method table for CTaskWorm (vtable at 0x6644C8, 20 slots).
///
/// CTaskWorm overrides 14 of the 20 inherited CTask/CGameTask slots;
/// 6 slots pass through unchanged. Slot layout by vtable byte offset:
///
/// ```text
/// 0x00 WriteReplayState  0x04 Free             0x08 HandleMessage
/// 0x0C GetEntityData     0x10-0x18 inherited   0x1C OnContactEntity
/// 0x20 OnWormPush        0x24 OnLandBounce     0x28 OnLandSlide
/// 0x2C OnSink            0x30 inherited        0x34 OnKilled
/// 0x38 SetState          0x3C CheckPendingAction 0x40 IsNotOnRope
/// 0x44 inherited         0x48 GetTeamIndex     0x4C inherited
/// ```
#[repr(C)]
pub struct CTaskWormVTable {
    /// [0] 0x0050CAA0: WriteReplayState — serializes worm state to a replay stream;
    ///   writes entity type byte 0x12, then packs field groups based on current state
    pub write_replay_state: unsafe extern "thiscall" fn(*mut CTaskWorm, *mut u8),
    /// [1] 0x0050C7E0: Free — calls inner destructor, then `_free(this)` if flags & 1
    pub free: unsafe extern "thiscall" fn(*mut CTaskWorm, u8) -> *mut CTaskWorm,
    /// [2] 0x00510B40: HandleMessage — processes all TaskMessages sent to this worm
    pub handle_message:
        unsafe extern "thiscall" fn(*mut CTaskWorm, *mut CTask, u32, u32, *const u8),
    /// [3] 0x00516780: GetEntityData — returns worm data by query code:
    ///   0x7D0=(pos_x, pos_y), 0x7D1/0x7D2=collision test, 0x7D4=full state dump
    pub get_entity_data: unsafe extern "thiscall" fn(*mut CTaskWorm, u32, u32, *mut u32) -> u32,
    /// [4]-[6]: Inherited CTask stubs (not overridden by CTaskWorm)
    pub _inherited_4_to_6: [*const (); 3],
    /// [7] 0x0050D5D0: OnContactEntity — handles physical contact with another entity;
    ///   worm-on-dying-worm → apply crush damage; projectile close-pass → SetState(0x80)
    pub on_contact_entity:
        unsafe extern "thiscall" fn(*mut CTaskWorm, *mut CGameTask, u32) -> u32,
    /// [8] 0x0050D9A0: OnWormPush — post-contact worm-worm push impulse;
    ///   deduplicates via a recent-collision table, then adjusts pos_x or pos_y
    pub on_worm_push: unsafe extern "thiscall" fn(*mut CTaskWorm, *mut CGameTask, u32) -> u32,
    /// [9] 0x0050D810: OnLandBounce — worm lands on terrain; plays thud sound, bounce physics
    pub on_land_bounce: unsafe extern "thiscall" fn(*mut CTaskWorm),
    /// [10] 0x0050D820: OnLandSlide — secondary landing callback; sliding/friction physics
    pub on_land_slide: unsafe extern "thiscall" fn(*mut CTaskWorm),
    /// [11] 0x0050D570: OnSink — worm sinks in water/acid; applies (dx, dy) displacement,
    ///   transitions to drowning state (0x7F) unless already flying/rope/dead
    pub on_sink: unsafe extern "thiscall" fn(*mut CTaskWorm, i32, i32) -> u32,
    /// [12]: Inherited (vtable30 from CGameTask — not overridden)
    pub _inherited_12: *const (),
    /// [13] 0x0050D3B0: OnKilled — worm death; plays death sound (0x3A),
    ///   transitions to dead state (0x81 or 0x89) based on game round count
    pub on_killed: unsafe extern "thiscall" fn(*mut CTaskWorm),
    /// [14] 0x0050E850: SetState — worm state machine; handles all state transitions
    ///   (0x65=idle, 0x67=active turn, 0x7F=drowning, 0x80=hurt, 0x81/0x86=dead, …)
    pub set_state: unsafe extern "thiscall" fn(*mut CTaskWorm, u32),
    /// [15] 0x00516900: CheckPendingAction — if field +0xBC is set, calls SetState(0x73)
    pub check_pending_action: unsafe extern "thiscall" fn(*mut CTaskWorm),
    /// [16] 0x00516920: IsNotOnRope — returns true if worm state != 0x7C (rope-swinging)
    pub is_not_on_rope: unsafe extern "thiscall" fn(*const CTaskWorm) -> bool,
    /// [17]: Inherited (vtable44 from CGameTask — not overridden)
    pub _inherited_17: *const (),
    /// [18] 0x005168F0: GetTeamIndex — returns worm's team index (field +0xFC)
    pub get_team_index: unsafe extern "thiscall" fn(*const CTaskWorm) -> u32,
    /// [19]: Inherited (vtable4C from CGameTask — not overridden)
    pub _inherited_19: *const (),
}

const _: () = assert!(core::mem::size_of::<CTaskWormVTable>() == 20 * 4);

/// Worm entity task — the primary playable character in WA.
///
/// Extends CGameTask (0xFC bytes) with worm identity, physics overrides, and
/// per-worm state. Total size: 0x3FC bytes.
///
/// Constructor: 0x50BFB0 (stdcall, 5 params):
///   this, parent_task, team_index, worm_index, init_data_ptr
///
/// Vtable at 0x6644C8. Class type byte: 0x12.
///
/// # Important fields in the CGameTask base
/// The worm state field lives at **offset +0x44** (inside `base._unknown_30`).
/// Use [`CTaskWorm::state`] to read it without pointer arithmetic.
///
/// Source: Ghidra decompilation of 0x50BFB0, vtable analysis of 0x6644C8,
///         wkJellyWorm CTaskWorm.h
#[repr(C)]
pub struct CTaskWorm {
    /// 0x00–0xFB: CGameTask base (position, velocity, sound emitter, etc.)
    pub base: CGameTask,

    /// 0xFC: Team index (0-based); 3rd constructor param
    pub team_index: u32,
    /// 0x100: Worm index within team (0-based); 4th constructor param
    pub worm_index: u32,
    /// 0x104: Unknown flag (checked in OnContactEntity)
    pub _unknown_104: u32,
    /// 0x108–0x10F: Unknown
    pub _unknown_108: [u8; 8],
    /// 0x110–0x137: Ten u32s copied from spawn init_data (5th constructor param)
    pub spawn_params: [u32; 10],
    /// 0x138–0x14F: Unknown
    pub _unknown_138: [u8; 0x18],
    /// 0x150: Unknown (slot 9 in GetEntityData query 0x7D4 output)
    pub _unknown_150: u32,
    /// 0x154: Unknown (rope-related; cleared in some SetState transitions)
    pub _unknown_154: u32,
    /// 0x158: Worm pool slot index in DDGame (assigned from pool at construction)
    pub slot_id: u32,
    /// 0x15C–0x247: Unknown
    pub _unknown_15c: [u8; 0xEC],
    /// 0x248–0x25B: Aiming / weapon data (slots 4–8 in GetEntityData query 0x7D4)
    pub _unknown_248: [u32; 5],
    /// 0x25C–0x283: Unknown
    pub _unknown_25c: [u8; 0x28],
    /// 0x284–0x28B: Aiming data (slots 5–6 in GetEntityData query 0x7D4)
    pub _unknown_284: [u32; 2],
    /// 0x28C–0x2EF: Unknown
    pub _unknown_28c: [u8; 0x64],
    /// 0x2F0: Worm name, null-terminated (max 17 chars, from spawn init_data+3)
    pub worm_name: [u8; 0x11],
    /// 0x301: Country / team name from scheme, null-terminated (max 17 chars)
    pub country_name: [u8; 0x11],
    /// 0x312–0x367: Unknown (rope string, state history, etc.)
    pub _unknown_312: [u8; 0x56],
    /// 0x368: Animator / controller object (dispatched via vtable for state animations)
    pub animator: *mut u8,
    /// 0x36C–0x3FB: Unknown (rope anchor, weapon-specific data, etc.)
    pub _unknown_36c: [u8; 0x90],
}

const _: () = assert!(core::mem::size_of::<CTaskWorm>() == 0x3FC);

/// Message-subscription filter task — routes messages selectively to child tasks.
///
/// CTaskFilter is a CTask subclass that overrides HandleMessage to only forward
/// messages whose type is marked in a 100-entry boolean subscription table. Each
/// CTaskFilter instance subscribes to a specific set of message IDs at construction
/// time; all other messages are silently dropped before reaching the subtree.
///
/// **Role in the task tree**: CTaskTeam creates multiple CTaskFilter children per
/// team during construction. Each filter represents a different event-routing path
/// (e.g., movement, UI, game-flow, weather). Messages from CTaskTurnGame propagate
/// down through these filters, which gate access to their subtrees.
///
/// **Allocation size**: 0xB4 bytes (via operator new in factory functions).
///
/// **Constructor**: `CTaskFilter__Constructor` (0x54F3D0, thiscall):
/// - `init_val_1c`: stored at CTask+0x1C (role unknown)
/// - `parent_task`: parent in the task tree (determines shared_data)
///
/// **Key vtable methods** (vtable at 0x669DAC):
/// - [2] `CTaskFilter__HandleMessage` (0x54F4A0): checks subscription table, forwards
///   only if `msg_type < 100 && subscription_table[msg_type] != 0`
/// - [7] `CTaskFilter__SubscribeAll` (0x54F390): sets all 100 entries to 1
/// - [8] `CTaskFilter__Subscribe` (0x54F370): sets `subscription_table[msg_id] = 1`
///
/// **Four factory functions** (all called by `CTaskTeam__Constructor_Maybe` 0x550E70):
/// - `FUN_00552030`: subscribes to messages 0, 1, 3, 5
/// - `FUN_005520D0`: subscribes to messages 0, 1, 2, 3, 0x15, 0x18, 0x1C
/// - `FUN_00552190`: subscribes to messages 0, 1, 2, 3, 5, 0x15, 0x17, 0x1C, 0x2C–0x2E, 0x4B,
///   and optionally 0x0E (if `GameInfo+0xD778 < -1`)
/// - `CTaskTeam__CreateWeatherFilter` (0x552960): subscribes to 1, 2, 3, 0x54, then
///   spawns `CTaskCloud` children using randomised positions (only if `DDGame+0x777C == 0`)
#[repr(C)]
pub struct CTaskFilter {
    /// 0x00–0x2F: CTask base.
    ///
    /// Notable base fields set by CTaskFilter__Constructor:
    /// - CTask+0x18 (`_unknown_18`): set to 0
    /// - CTask+0x1C (`_unknown_1c`): set to `init_val_1c` constructor param
    /// - CTask+0x20 (`_unknown_20`): set to 7 (task type / mode constant)
    pub base: CTask,
    /// 0x30–0x93: Boolean subscription table, indexed by message type ID (0–99).
    ///
    /// `subscription_table[id] != 0` means this filter will forward messages of
    /// that type. Cleared to 0 at construction, then populated by Subscribe/SubscribeAll
    /// calls. Max 100 distinct message IDs (IDs >= 100 always pass through).
    pub subscription_table: [u8; 100],
    /// 0x94–0xB3: Unknown (present in 0xB4-byte allocation; not set by constructor).
    pub _unknown_94: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<CTaskFilter>() == 0xB4);

/// Airstrike / weather cloud task.
///
/// Extends CTask directly (not CGameTask). Clouds drift horizontally with wind,
/// scroll on a parallax layer, and render as a single sprite.
///
/// Allocation: 0x74 bytes (operator new in CTaskTeam__CreateWeatherFilter).
/// Constructor: 0x5482E0 (usercall ESI=this, EAX=parent).
/// Vtable: 0x669D38. Class type byte: 0x17.
///
/// Three cloud sizes chosen by `cloud_type` param (0/1/2):
/// - type 0: sprite 0x268 (large),  vel_y 0x200
/// - type 1: sprite 0x269 (medium), vel_y 0x166
/// - type 2: sprite 0x26A (small),  vel_y 0xCC
///
/// Source: Ghidra decompilation of 0x5482E0 (constructor) and
///         0x5484C0 (HandleMessage update + render branches).
#[repr(C)]
pub struct CTaskCloud {
    /// 0x00–0x2F: CTask base
    pub base: CTask,
    /// 0x30: Parallax scroll layer depth (Fixed; 0x190000 = 25.0 at spawn,
    /// decrements by 1 each cloud spawned in a batch)
    pub layer_depth: Fixed,
    /// 0x34: Y position (Fixed 16.16); updated each frame: pos_y += vel_y
    pub pos_y: Fixed,
    /// 0x38: Y velocity (Fixed 16.16; set by cloud type: large=0x200, medium=0x166, small=0xCC)
    pub vel_y: Fixed,
    /// 0x3C: Sprite ID passed to DrawSpriteLocal (0x268=large, 0x269=medium, 0x26A=small)
    pub sprite_id: u32,
    /// 0x40: X position (Fixed 16.16); wraps at landscape bounds each frame
    pub pos_x: Fixed,
    /// 0x44: Unknown (set from EDI at call site)
    pub _unknown_44: u32,
    /// 0x48: X velocity base (Fixed 16.16)
    pub vel_x: Fixed,
    /// 0x4C: Current wind acceleration (Fixed); converges toward wind_target each frame
    pub wind_accel: Fixed,
    /// 0x50: Target wind speed (Fixed); set by message 0x54 (wind-change event)
    pub wind_target: Fixed,
    /// 0x54–0x73: Unknown
    pub _unknown_54: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<CTaskCloud>() == 0x74);

impl CTaskWorm {
    /// Returns the worm's current state code (lives at offset +0x44, inside the
    /// CGameTask base's `_unknown_30` padding region).
    ///
    /// Known states: `0x65`=idle, `0x67`=active turn, `0x7F`=drowning,
    /// `0x80`=hurt, `0x81`/`0x86`=dead, `0x87`=dead variant, `0x8B`=unknown.
    pub fn state(&self) -> u32 {
        // SAFETY: offset 0x44 is within CGameTask._unknown_30 (0x30..0x84).
        // Aligned to 4 bytes; repr(C) guarantees no reordering.
        unsafe { *((self as *const CTaskWorm as *const u8).add(0x44) as *const u32) }
    }

    /// Returns a reference to the vtable.
    ///
    /// # Safety
    /// The vtable pointer at offset 0 must be the genuine CTaskWorm vtable
    /// (0x6644C8 in Ghidra, rebased at runtime).
    pub unsafe fn vtable(&self) -> &'static CTaskWormVTable {
        &*(self.base.base.vtable as *const CTaskWormVTable)
    }
}

/// Land mine entity task.
///
/// Extends CGameTask (0xFC bytes). Mines sit on the terrain and arm after
/// placement; they detonate on contact once armed.
///
/// Constructor: 0x506660 (stdcall).
/// Vtable: 0x6643E8. Class type byte: 0x08.
///
/// Source: Ghidra decompilation of 0x506660 (constructor) and
///         0x5072E0 (HandleMessage, msg 2/0x15/0x1C/0x4B branches).
#[repr(C)]
pub struct CTaskMine {
    /// 0x00–0xFB: CGameTask base (pos at 0x84/0x88, speed at 0x90/0x94)
    pub base: CGameTask,
    /// 0xFC–0x10F: Unknown mine flags
    pub _unknown_fc: [u8; 0x14],
    /// 0x110: Object pool slot index (assigned from DDGame+0x3600 pool)
    pub slot_id: u32,
    /// 0x114–0x11B: Unknown
    pub _unknown_114: [u8; 8],
    /// 0x11C: Fuse timer (signed i32).
    /// Negative = just placed / disarmed.
    /// 0 = armed (will trigger on contact).
    /// Positive = countdown ticks remaining.
    pub fuse_timer: i32,
    /// 0x120–0x123: Unknown (init data param_3[0])
    pub _unknown_120: u32,
    /// 0x124: Owner team index (param_3[6]; -1 = no owner)
    pub owner_team: i32,
}

const _: () = assert!(core::mem::size_of::<CTaskMine>() == 0x128);

/// Exploding oil drum entity task.
///
/// Extends CGameTask (0xFC bytes). Oil drums roll on terrain and explode
/// when hit enough times (health decrements per impact).
///
/// Constructor: 0x504AF0 (thiscall).
/// Vtable: 0x664338. Class type byte: 0x1E.
///
/// Source: Ghidra decompilation of 0x504AF0 (constructor) and
///         0x5050B0 (HandleMessage, msg 2/0x1C branches).
#[repr(C)]
pub struct CTaskOilDrum {
    /// 0x00–0xFB: CGameTask base (pos at 0x84/0x88, speed at 0x90/0x94)
    pub base: CGameTask,
    /// 0xFC: Triggered flag — set to 1 on first impact, starts smoke/fire
    pub triggered: u32,
    /// 0x100: Object pool slot index
    pub slot_id: u32,
    /// 0x104: Unknown
    pub _unknown_104: u32,
    /// 0x108: Health (starts at 0x32 = 50; decremented on damage)
    pub health: u32,
    /// 0x10C: Rolling animation counter (increments by 0x4000 per frame while moving)
    pub roll_counter: u32,
}

const _: () = assert!(core::mem::size_of::<CTaskOilDrum>() == 0x110);

impl CTaskOilDrum {
    /// Returns true if the drum is on fire (flag at CGameTask+0xB0, inside _unknown_98).
    ///
    /// # Safety
    /// `self` must be a valid, fully-constructed CTaskOilDrum.
    pub unsafe fn on_fire(&self) -> bool {
        let ptr = (self as *const CTaskOilDrum as *const u8).add(0xB0);
        *(ptr as *const u32) != 0
    }
}

/// Per-team state-tracker task — one instance per team, child of CTaskTurnGame.
///
/// Tracks per-team data: which team number it represents, how many worms were
/// spawned, and a weapon/item slot table.  Registered in the SharedData hash
/// table with type code 0x15 (21).
///
/// Inheritance: extends CTask directly (CTask base at +0x00).
/// class_type = 10.  Allocation: 0x460 bytes via `operator new` (0x5C0AB8).
/// Constructor: `CTaskTeam__Constructor` (0x555BB0).
/// Primary vtable: `CTaskTeam__vtable2` (0x00669EE4).
/// Secondary interface vtable: 0x00669F00 (at object +0x30).
///
/// Key constructor logic (0x555BB0):
///   - `team_index` at +0x38: 1-based team number (passed as param_3)
///   - `_item_slots[0..99]` at +0x88: up to 100 weapon/item IDs loaded from scheme
///   - `worm_count` at +0x218: number of CTaskWorm children constructed (1-indexed)
///   - SharedData node (0x30 bytes) registered with key = (team_index, type=0x15)
#[repr(C)]
pub struct CTaskTeam {
    /// 0x00–0x2F: CTask base (vtable, parent, children, shared_data, ddgame, …)
    pub base: CTask,
    /// 0x30: Secondary interface vtable pointer (Ghidra 0x00669F00)
    pub _secondary_vtable: u32,
    /// 0x34: Unknown — observed to hold the same value as `team_index` in all runs.
    pub _unknown_34: u32,
    /// 0x38: Team number, **1-based**.  Matches TeamArenaState slot index.
    pub team_index: u32,
    /// 0x3C: Unknown — observed 0 for team 1, 1 for team 2.
    pub _unknown_3c: u32,
    /// 0x40: Unknown signed integer — observed −40 for team 1, −20 for team 2.
    pub _unknown_40: i32,
    /// 0x44: Unknown (always 0 in observed runs).
    pub _unknown_44: u32,
    /// 0x48: Number of living worms remaining on this team.
    /// Decrements as worms are eliminated; equals `worm_count` at game start.
    pub alive_worm_count: u32,
    /// 0x4C–0x5B: Unknown.
    pub _unknown_4c: [u8; 16],
    /// 0x5C: Unknown — consistently 11 across teams and runs.
    pub _unknown_5c: u32,
    /// 0x60: Index of the last weapon launched by this team (0 = none launched yet).
    /// Field name confirmed by wkJellyWorm: `lastLaunchedWeapon_dword60`.
    pub last_launched_weapon: u32,
    /// 0x64–0x87: Unknown.
    pub _unknown_64: [u8; 36],
    /// 0x88–0x217: Unknown region (100 DWORDs).  Observed non-zero values at the start
    /// (+0x88 = team_index, +0x8C/+0x90 = 3) in a 2-worm bot game; purpose unclear.
    pub _unknown_88: [u32; 100],
    /// 0x218: Number of CTaskWorm children constructed for this team.
    pub worm_count: u32,
    /// 0x21C–0x3EB: Unknown.
    pub _unknown_21c: [u8; 0x1D0],
    /// 0x3EC–0x3F3: Unknown flags (observed: 1, 1 then 0xFFFFFFFF×3 at +0x3F4).
    pub _unknown_3ec: [u32; 2],
    /// 0x3F4–0x3FF: Sentinel values (observed: 0xFFFFFFFF in all runs).
    pub _sentinels_3f4: [u32; 3],
    /// 0x400–0x403: Unknown.
    pub _unknown_400: u32,
    /// 0x404: X position (Fixed16.16) — per-team, likely spawn or last-worm position.
    pub pos_x: Fixed,
    /// 0x408: Y position (Fixed16.16) — per-team, likely spawn or last-worm position.
    pub pos_y: Fixed,
    /// 0x40C: Unknown Fixed16.16 value — same for all teams (observed ≈ 666.89).
    pub _unknown_40c: Fixed,
    /// 0x410: Unknown (observed 1 in all runs).
    pub _unknown_410: u32,
    /// 0x414–0x45F: Unknown.
    pub _unknown_414: [u8; 0x4C],
}

const _: () = assert!(core::mem::size_of::<CTaskTeam>() == 0x460);

/// Fire/flame entity task.
///
/// Extends CTask (not CGameTask) — no physics body.
/// class_type = 0x18. Allocated 0xD8 bytes.
/// Constructor: CTaskFire__Constructor (0x54F4C0).
/// vtable: CTaskFire__vtable (0x00669DD8).
///
/// One CTaskFire is spawned per flame sprite.  The `timer` field starts
/// at 0xFFFF and counts down each frame; when it reaches zero the fire
/// dies.  `lifetime` at +0xB1 is a signed byte: 0xFF (= -1i8) means alive,
/// 0 means the task is being destroyed.
#[repr(C)]
pub struct CTaskFire {
    /// 0x00-0x2F: CTask base
    pub base: CTask,
    /// 0x30: spread counter (incremented while fire is spreading)
    pub spread_counter: i32,
    /// 0x34: frame countdown; starts at 0xFFFF, decrements each ProcessFrame
    pub timer: i32,
    /// 0x38: random seed / initial offset for sprite variation
    pub rand_offset: u32,
    /// 0x3C: burn rate / intensity (higher = more damage per frame)
    pub burn_rate: u32,
    pub _unknown_40: u32,
    /// 0x44: spawn X position (Fixed 16.16)
    pub spawn_x: crate::fixed::Fixed,
    /// 0x48: spawn Y position (Fixed 16.16)
    pub spawn_y: crate::fixed::Fixed,
    pub _unknown_4c: [u8; 0x24],
    /// 0x70: absolute tick (frame counter) when this flame was spawned
    pub spawn_time: u32,
    pub _unknown_74: u32,
    /// 0x78-0xA7: per-frame spawn parameter table (12 DWORDs)
    pub spawn_params: [u32; 12],
    /// 0xA8: slot index in the fire-object pool
    pub slot_index: u32,
    pub _unknown_ac: u32,
    pub _flags_b0: u8,
    /// 0xB1: lifetime byte; -1 (0xFF as i8) = alive, 0 = dying/dead
    pub lifetime: i8,
    pub _unknown_b2: [u8; 0x26],
}

const _: () = assert!(core::mem::size_of::<CTaskFire>() == 0xD8);

/// Embedded intermediate game-context sub-object within `CTaskTurnGame`.
///
/// This is the memory region at `CTaskTurnGame+0x30..+0xDB` (0xAC bytes).
/// It is initialised by `CTaskTeam__Constructor_Maybe` (0x550EB0), which:
///   1. Calls `CTask::Constructor(this, nullptr, ddgame)`
///   2. Sets the primary vtable to 0x669E34 and class_type to 5
///   3. Sets a **secondary interface vtable** pointer (Ghidra 0x669C44) at +0x30
///      inside the object (i.e. `TurnGameCtx` base +0x00)
///   4. Copies `landscape_height = DDGame+0x5E0` as Fixed16.16 to both
///      `land_height` and `land_height_2`
///   5. Writes -1 sentinels to `_sentinel_18`, `_sentinel_28`, `_sentinel_38`
///   6. Writes `team_count = *(byte*)(GameInfo+0x44C)` to `team_count`
///
/// `CTaskTurnGame__Constructor` (0x55B2A0) then overrides the primary vtable
/// to 0x669F70 and class_type to 6 but **leaves this sub-object intact**.
///
/// The three -1 sentinels at offsets 0x18 / 0x28 / 0x38 are evenly spaced
/// 0x10 bytes apart. Their role is unknown — candidates are water-level
/// Y-coordinates (initial -1 = "no water"), zone boundaries, or AI markers.
///
/// The final 0x44 bytes (0x68–0xAB, `_unknown_68`) are not initialised by
/// either constructor; they appear to be set by `FUN_005514d0` or similar
/// helpers called later. Observed runtime values: +0xA0 = 15, +0xA4/+0xA8
/// are heap pointers.
#[repr(C)]
pub struct TurnGameCtx {
    /// +0x00 (= CTaskTurnGame+0x30): Secondary interface vtable pointer.
    /// Ghidra: 0x00669C44.  Set by both constructors; always 0x669C44 at runtime.
    pub _secondary_vtable: u32,
    /// +0x04: Unknown — not set by constructors (remains 0).
    pub _unknown_04: u32,
    /// +0x08–0x0F: Unknown — explicitly zeroed by constructor.
    pub _unknown_08: [u32; 2],
    /// +0x10: Landscape height as Fixed16.16.  `DDGame+0x5E0 << 16`.
    pub land_height: Fixed,
    /// +0x14: Landscape height duplicate — same value as `land_height`.
    pub land_height_2: Fixed,
    /// +0x18: Sentinel, always -1 at construction.  Role unknown.
    pub _sentinel_18: i32,
    /// +0x1C–0x27: Unknown — explicitly zeroed by constructor.
    pub _unknown_1c: [u32; 3],
    /// +0x28: Sentinel, always -1 at construction.  Role unknown.
    pub _sentinel_28: i32,
    /// +0x2C–0x37: Unknown — explicitly zeroed by constructor.
    pub _unknown_2c: [u32; 3],
    /// +0x38: Sentinel, always -1 at construction.  Role unknown.
    pub _sentinel_38: i32,
    /// +0x3C–0x4B: Unknown — explicitly zeroed by constructor.
    pub _unknown_3c: [u32; 4],
    /// +0x4C: Number of teams.  `*(byte*)(GameInfo+0x44C)` at construction time.
    pub team_count: u32,
    /// +0x50–0x64: Unknown — explicitly zeroed by constructor (param_1[0x20..0x25]).
    pub _unknown_50: [u32; 6],
    /// +0x68–0x9F: Unknown — all explicitly zeroed by `FUN_005514d0` during game
    /// setup. Purpose unknown; may be per-team or per-worm state slots.
    pub _unknown_68: [u32; 14],
    /// +0xA0: Return value of `FUN_00525f50(0)` stored here during game setup.
    /// Observed value: 15. `FUN_00525f50` is a slot-allocation helper; this may
    /// be a pool slot index or a pre-computed game-state token.
    pub _slot_d0: u32,
    /// +0xA4: `DDDisplay` textbox handle — created by `DDDisplay__ConstructTextbox`
    /// with params `(buf, -1280, 2)` if `DDGame+0x7EF8 != 0` (display active).
    /// Likely the HUD timer textbox.  NULL when display is disabled.
    pub _hud_textbox_a: u32,
    /// +0xA8: `DDDisplay` textbox handle — created with params `(buf, 8, 4)`.
    /// Likely a secondary HUD element.  NULL when display is disabled.
    pub _hud_textbox_b: u32,
}

const _: () = assert!(core::mem::size_of::<TurnGameCtx>() == 0xAC);

/// Root turn-controller task — one instance per game, parent of the entire entity tree.
///
/// Every worm, team, projectile, and environment task is a child (direct or indirect)
/// of this node.  `CTaskTurnGame` drives the turn loop: it processes 50 game frames
/// per second via `CTaskTurnGame__TurnManager_ProcessFrame` (0x55FDA0), which is
/// called from HandleMessage case 2 (FrameFinish).
///
/// Inheritance: CTask → CTaskTeam → CTaskTurnGame.  Class type 6.
/// Constructor: `CTaskTurnGame__Constructor` (0x55B2B1).
/// vtable: `CTaskTurnGame__vtable` (0x00669F70), 20 slots.
/// Total size: 0x2E0 bytes.
///
/// Key vtable slots:
///   [0] 0x55B5E0 — task-tree state snapshot serialiser
///   [1] 0x55B540 — destructor / Free
///   [2] 0x55DC00 — HandleMessage (30+ message types)
///   [3] 0x5612E0 — HUD data query (responds to msg 0x7D3)
///
/// All timers decrement by 20 ms per frame (= 1000 ms / 50 fps).
#[repr(C)]
pub struct CTaskTurnGame {
    /// 0x00-0x2F: CTask base (vtable, parent, children, shared_data, ddgame, …)
    pub base: CTask,
    /// 0x30–0xDB: Embedded `TurnGameCtx` sub-object (0xAC bytes).
    /// See [`TurnGameCtx`] for field details.
    pub game_ctx: TurnGameCtx,

    // ---- CTaskTurnGame-specific fields (0xDC onwards) ----

    pub _unknown_dc: u32,
    pub _unknown_e0: u32,
    /// 0xE4: turn-seed: ~(DDGame+0x45EC % 9000), used for random initialisation.
    pub _turn_seed: u32,
    /// 0xE8: number of teams in this game (copy of DDGame+0x44C).
    pub num_teams: u32,
    pub _unknown_ec: [u8; 0x1C],
    /// 0x108: "worm active" flag — non-zero while the current worm is shooting or
    /// moving.  While non-zero, `turn_timer` is paused.
    /// A copy is also written to DDGame+0x7234 at construction.
    pub worm_active: u32,
    pub _unknown_10c: [u8; 0x20],
    /// 0x12C: active team index, **1-based** (0 = no active team).
    /// Used to index per-team sound tables in DDGame (stride 0xF0 at DDGame+0x774C).
    pub current_team: u32,
    /// 0x130: active worm index within the current team, **0-based** (stride 0x9C).
    pub current_worm: u32,
    /// 0x134: arena team index used for TeamArenaState lookups.
    pub arena_team: u32,
    /// 0x138: arena worm index used for TeamArenaState lookups.
    pub arena_worm: u32,
    pub _unknown_13c: u32,
    /// 0x140: set to 1 at construction; role unclear.
    pub _init_140: u32,
    pub _unknown_144: u32,
    pub _unknown_148: u32,
    /// 0x14C: set to 1 at construction and reset by turn-flow restart; role unclear.
    pub _init_14c: u32,
    /// 0x150: set to 1 when the active worm's firing phase ends and retreat begins.
    pub turn_ended: u32,
    /// 0x154: set to 1 when the scheme turn time is 0 (no time limit).
    pub no_time_limit: u32,
    pub _unknown_158: [u8; 0x14],
    /// 0x16C: set to 1 once the "5 seconds left" warning sound has played.
    pub warning_sound_played: u32,
    pub _unknown_170: u32,
    /// 0x174: set while a worm is transitioning out of a special state (e.g. landing
    /// after a fly animation); cleared once settled.
    pub _worm_state_transition: u32,
    /// 0x178: retreat-phase countdown (ms).  Initialised from `retreat_time_max` when
    /// firing ends; decrements 20 ms/frame.  Dispatches message 0x35 at zero.
    pub retreat_timer: i32,
    /// 0x17C: initial retreat duration (ms) loaded from the scheme at turn start.
    pub retreat_time_max: i32,
    /// 0x180: last tick when the displayed "seconds remaining" value was updated.
    pub last_second_tick: u32,
    /// 0x184: idle-phase timer (ms).  Initialised to `scheme_turn_time × 1000`.
    /// Decrements 20 ms/frame **only while `worm_active == 0`** (timer pauses when
    /// the worm is moving/shooting).  When it reaches zero, sudden-death is triggered.
    /// This is NOT the per-turn HUD countdown — see `turn_timer` at +0x18C instead.
    pub idle_timer: i32,
    /// 0x188: smoothed display copy of `turn_timer`; tracks it at ≤ 0xCCC ms/frame
    /// to avoid visual jumping on the HUD.
    pub turn_timer_display: i32,
    /// 0x18C: per-turn countdown timer (ms) — this is what the HUD shows.
    /// Decrements 20 ms/frame every game frame.  When it reaches zero,
    /// `FUN_0055f4f0` fires (turn ends / retreat begins).
    pub turn_timer: i32,
    pub _unknown_190: [u8; 0x8],
    /// 0x198: turn-timer visual pulse intensity (Fixed 16.16); grows as time runs low.
    pub timer_flash: u32,
    pub _unknown_19c: [u8; 0x138],
    /// 0x2D4: frame count accumulated while this team held the active worm.
    pub active_worm_frames: u32,
    /// 0x2D8: frame count accumulated during the retreat phase.
    pub retreat_frames: u32,
    /// 0x2DC: timer scale factor derived from scheme (2, 3, or 4; default 4).
    pub _timer_scale: u32,
}

const _: () = assert!(core::mem::size_of::<CTaskTurnGame>() == 0x2E0);

// ============================================================
// Derived task overlays — for accessing task-specific fields
// beyond or within CGameTask that differ per task type.
// ============================================================

/// Bungee trail rendering task fields.
///
/// Used by DrawBungeeTrail (0x500720). Fields at 0xBC-0xE4 overlap with
/// CGameTask's `_unknown_98` region — different task types may use these
/// offsets for different purposes.
///
/// Cast a task pointer to this type when you know it's a bungee trail task.
#[repr(C)]
pub struct BungeeTrailTask {
    /// 0x00-0x2F: CTask base
    pub base: CTask,
    /// 0x30-0x83: Unknown
    pub _unknown_30: [u8; 0x54],
    /// 0x84: X position in fixed-point
    pub pos_x: Fixed,
    /// 0x88: Y position in fixed-point
    pub pos_y: Fixed,
    /// 0x8C-0xBB: Unknown
    pub _unknown_8c: [u8; 0x30],
    /// 0xBC: Trail visible flag (set by InitWormTrail when Bungee is used)
    pub trail_visible: i32,
    /// 0xC0: Trail start X position
    pub trail_start_x: i32,
    /// 0xC4: Trail start Y position
    pub trail_start_y: i32,
    /// 0xC8-0xCF: Unknown
    pub _unknown_c8: [u8; 8],
    /// 0xD0: Number of trail segments
    pub segment_count: i32,
    /// 0xD4-0xE3: Unknown
    pub _unknown_d4: [u8; 0x10],
    /// 0xE4: Pointer to segment data array (8 bytes per segment: 4 padding + 4 angle)
    pub segment_data: *const u8,
    /// 0xE8: Sound emitter sub-object
    pub sound_emitter: SoundEmitter,
}

const _: () = assert!(core::mem::size_of::<BungeeTrailTask>() == 0xFC);

/// Weapon aiming task fields.
///
/// Used by DrawCrosshairLine (0x5197D0). Fields at 0x258+ are in the derived
/// class region beyond CGameTask (0xFC). The exact class name is unknown.
///
/// Cast a task pointer to this type when you know it's a worm/weapon aiming task.
#[repr(C)]
pub struct WeaponAimTask {
    /// 0x00-0xFB: CGameTask base
    pub game_task: CGameTask,
    /// 0xFC-0x257: Unknown derived fields
    pub _unknown_fc: [u8; 0x258 - 0xFC],
    /// 0x258: Aiming active flag (nonzero = crosshair visible)
    pub aim_active: i32,
    /// 0x25C-0x263: Unknown
    pub _unknown_25c: [u8; 8],
    /// 0x264: Current aim angle (used for trig lookup)
    pub aim_angle: u32,
    /// 0x268-0x323: Unknown
    pub _unknown_268: [u8; 0x324 - 0x268],
    /// 0x324: Aim range offset (added to DDGame crosshair scale)
    pub aim_range_offset: i32,
}
