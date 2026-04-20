use super::base::CTask;
use crate::FieldRegistry;
use openwa_core::fixed::Fixed;

crate::define_addresses! {
    class "CGameTask" {
        /// CGameTask vtable - extends CTask vtable with 12 more methods
        vtable CGAMETASK_VTABLE = 0x006641F8;
        // Sound emitter vtable now defined via #[vtable(...)] on SoundEmitterVTable
        /// CGameTask constructor - calls CTask ctor, sets physics defaults
        ctor/Stdcall CGAMETASK_CONSTRUCTOR = 0x004FED50;
        /// CGameTask::vtable0 override
        vmethod CGAMETASK_VT0 = 0x004FF1C0;
        /// CGameTask::Free override
        vmethod CGAMETASK_VT1_FREE = 0x004FEF10;
        /// CGameTask::HandleMessage override
        vmethod CGAMETASK_VT2_HANDLE_MESSAGE = 0x004FF280;
    }
}

/// Backward-compatible alias for the SoundEmitter vtable const.
pub const CGAMETASK_SOUND_EMITTER_VT: u32 = SOUND_EMITTER_VTABLE;

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
pub struct CGameTask<V: super::base::Vtable = *const core::ffi::c_void> {
    /// 0x00-0x2F: Base CTask fields
    pub base: CTask<V>,
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
    /// +0x04: Unknown field
    pub _unknown_04: u32,
    /// +0x08: Reference count — incremented when an ActiveSoundEntry holds this emitter,
    /// decremented when the entry is released.
    pub local_ref_count: i32,
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
#[openwa_game::vtable(size = 12, va = 0x00669CF8, class = "SoundEmitter")]
pub struct SoundEmitterVTable {
    /// GetPosition(this, out_x, out_y) — reads pos_x/pos_y via owner
    #[slot(0)]
    pub get_position: fn(this: *const SoundEmitter, out_x: *mut u32, out_y: *mut u32),
    /// GetPosition2 — reads CGameTask+0x38/0x3C
    #[slot(1)]
    pub get_position2: fn(this: *const SoundEmitter, out_x: *mut u32, out_y: *mut u32),
    /// Destructor
    #[slot(3)]
    pub destructor: fn(this: *mut SoundEmitter, flags: u32) -> *mut SoundEmitter,
    /// HandleMessage — sound queue manager
    #[slot(4)]
    pub handle_message:
        fn(this: *mut SoundEmitter, sender: u32, msg_type: u32, size: u32, data: u32),
}
