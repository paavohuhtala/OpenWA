//! Sound-emitter sub-object embedded in every [`WorldEntity`] via MSVC
//! multiple inheritance. The `this` pointer for its vtable methods points
//! to the start of the sub-object (entity +0xE8), not the WorldEntity.

use super::game_entity::WorldEntity;

/// Sound emitter sub-object embedded in WorldEntity via MSVC multiple inheritance.
///
/// Provides spatial audio support. The `this` pointer for its vtable methods
/// points to the start of this sub-object (WorldEntity+0xE8), not the WorldEntity.
#[repr(C)]
pub struct SoundEmitter {
    /// +0x00: Vtable pointer
    pub vtable: *const SoundEmitterVtable,
    /// +0x04: Unknown field
    pub _unknown_04: u32,
    /// +0x08: Reference count — incremented when an ActiveSoundEntry holds this emitter,
    /// decremented when the entry is released.
    pub local_ref_count: i32,
    /// +0x0C: Number of active local sounds
    pub local_sound_count: i32,
    /// +0x10: Back-pointer to containing WorldEntity
    pub owner: *mut WorldEntity,
}

const _: () = assert!(core::mem::size_of::<SoundEmitter>() == 0x14);

/// Vtable for the SoundEmitter sub-object (0x669CF8, 12 slots).
///
/// Slots [0]-[4] are the sound emitter's own interface.
/// Slots [5]-[11] are inherited BaseEntity base methods.
#[openwa_game::vtable(size = 12, va = 0x00669CF8, class = "SoundEmitter")]
pub struct SoundEmitterVtable {
    /// GetPosition(this, out_x, out_y) — reads pos_x/pos_y via owner
    #[slot(0)]
    pub get_position: fn(this: *const SoundEmitter, out_x: *mut u32, out_y: *mut u32),
    /// GetPosition2 — reads WorldEntity+0x38/0x3C
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

bind_SoundEmitterVtable!(SoundEmitter, vtable);
