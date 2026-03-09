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
