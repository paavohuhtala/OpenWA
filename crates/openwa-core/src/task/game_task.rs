use super::base::CTask;
use crate::fixed::Fixed;
use crate::FieldRegistry;

crate::define_addresses! {
    class "CGameTask" {
        /// CGameTask vtable - extends CTask vtable with 12 more methods
        vtable CGAMETASK_VTABLE = 0x0066_41F8;
        /// CGameTask sound emitter vtable (embedded sub-object at offset 0xE8)
        vtable CGAMETASK_SOUND_EMITTER_VT = 0x0066_9CF8;
        /// CGameTask constructor - calls CTask ctor, sets physics defaults
        ctor/Stdcall CGAMETASK_CONSTRUCTOR = 0x004F_ED50;
        /// CGameTask::vtable0 override
        vmethod CGAMETASK_VT0 = 0x004F_F1C0;
        /// CGameTask::Free override
        vmethod CGAMETASK_VT1_FREE = 0x004F_EF10;
        /// CGameTask::HandleMessage override
        vmethod CGAMETASK_VT2_HANDLE_MESSAGE = 0x004F_F280;
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
#[derive(FieldRegistry)]
#[repr(C)]
pub struct CGameTask {
    /// 0x00-0x2F: Base CTask fields
    pub base: CTask,
    /// 0x30-0x83: Subclass-specific data (84 bytes). Each CGameTask derivative
    /// uses this region differently:
    /// - CTaskWorm: weapon fire state (+0x30 type, +0x34/+0x38 subtypes, +0x3C flag)
    /// - CTaskFilter: boolean message subscription table (+0x30..+0x93)
    /// - CTaskMissile: spawn/physics parameters
    /// - CTaskTeam: secondary vtable pointer (+0x30)
    /// - CTaskCloud: parallax scroll depth (+0x30)
    /// Access via subclass accessor methods, not directly.
    pub subclass_data: [u8; 0x54],
    /// 0x84: X position in fixed-point
    pub pos_x: Fixed,
    /// 0x88: Y position in fixed-point
    pub pos_y: Fixed,
    /// 0x8C: Rotation angle (fixed-point 16.16).
    pub angle: Fixed,
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
