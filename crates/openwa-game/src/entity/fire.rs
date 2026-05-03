use super::base::BaseEntity;
use crate::FieldRegistry;
use crate::address::va;
use openwa_core::fixed::Fixed;

crate::define_addresses! {
    class "FireEntity" {
        ctor FIRE_ENTITY_CTOR = 0x0054F4C0;
    }
}

/// FireEntity vtable — 12 slots. Extends BaseEntity base (8 slots) with fire behavior.
///
/// Vtable at Ghidra 0x669DD8.
#[openwa_game::vtable(size = 12, va = 0x00669DD8, class = "FireEntity")]
pub struct FireEntityVtable {
    /// HandleMessage — processes fire messages.
    /// thiscall + 4 stack params, RET 0x10.
    #[slot(2)]
    pub handle_message: fn(
        this: *mut FireEntity,
        sender: *mut BaseEntity,
        msg_type: u32,
        size: u32,
        data: *const u8,
    ),
    /// ProcessFrame — per-frame fire update (countdown, spread, damage).
    /// thiscall + 1 stack param (flags), RET 0x4.
    #[slot(7)]
    pub process_frame: fn(this: *mut FireEntity, flags: u32),
}

/// Fire/flame entity entity.
///
/// Extends BaseEntity (not WorldEntity) — no physics body.
/// class_type = 0x18. Allocated 0xD8 bytes.
/// Constructor: FireEntity__Constructor (0x54F4C0).
/// vtable: FireEntity__vtable (0x00669DD8).
///
/// One FireEntity is spawned per flame sprite.  The `timer` field starts
/// at 0xFFFF and counts down each frame; when it reaches zero the fire
/// dies.  `lifetime` at +0xB1 is a signed byte: 0xFF (= -1i8) means alive,
/// 0 means the entity is being destroyed.
#[derive(FieldRegistry)]
#[repr(C)]
pub struct FireEntity {
    /// 0x00-0x2F: BaseEntity base
    pub base: BaseEntity<*const FireEntityVtable>,
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
    pub spawn_x: Fixed,
    /// 0x48: spawn Y position (Fixed 16.16)
    pub spawn_y: Fixed,
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

const _: () = assert!(core::mem::size_of::<FireEntity>() == 0xD8);

// Generate typed vtable method wrappers: handle_message(), process_frame().
bind_FireEntityVtable!(FireEntity, base.vtable);

/// 12-dword (0x30 bytes) init payload for [`FireEntity::Constructor`].
///
/// Populated on the caller's stack and passed by reference. The first four
/// dwords mirror the WeaponReleaseContext spawn fields; the middle three are
/// fixed flags/discriminators (`{0, 4, 1}` for the PlacedExplosive caller —
/// other callers may use different values); the trailing four come from the
/// active `WeaponFireParams` plus the worm's `team_index`. Field semantics
/// past the first four are speculative — the FireEntity ctor at 0x0054F4C0
/// would need RE'ing to confirm.
#[repr(C)]
pub struct FireEntityInit {
    /// 0x00: Spawn X (typically `ctx.spawn_x`).
    pub spawn_x: Fixed,
    /// 0x04: Spawn Y (typically `ctx.spawn_y`).
    pub spawn_y: Fixed,
    /// 0x08: Spawn offset X (typically `ctx.spawn_offset_x`).
    pub spawn_offset_x: Fixed,
    /// 0x0C: Spawn offset Y.
    pub spawn_offset_y: Fixed,
    /// 0x10: Zero in PlacedExplosive caller.
    pub _flag_10: u32,
    /// 0x14: Discriminator (= 4 in PlacedExplosive caller).
    pub kind: u32,
    /// 0x18: One in PlacedExplosive caller.
    pub _flag_18: u32,
    /// 0x1C: Sourced from `WeaponFireParams.collision_radius`.
    pub fp_collision_radius: Fixed,
    /// 0x20: Sourced from `WeaponFireParams._fp_02`.
    pub fp_02: i32,
    /// 0x24: Sourced from `WeaponFireParams.spread`.
    pub fp_spread: i32,
    /// 0x28: Sourced from `WeaponFireParams._fp_04`.
    pub fp_04: i32,
    /// 0x2C: Owner team index (`worm.team_index`).
    pub team_index: u32,
}

const _: () = assert!(core::mem::size_of::<FireEntityInit>() == 0x30);

/// Typed wrapper for `FireEntity::Constructor` (WA 0x0054F4C0,
/// `__stdcall(this, parent, init, flags) -> *mut FireEntity`, RET 0x10).
///
/// `this` must be a freshly-allocated FireEntity buffer (0xD8 bytes); the
/// constructor zero-fills the first 0xB8 bytes itself only when called from
/// some sites — `FireWeapon__PlacedExplosive` does the memset before this
/// call, so callers that go through here should follow the same pattern.
#[inline]
pub unsafe fn fire_entity_construct(
    this: *mut FireEntity,
    parent: *mut u8,
    init: *const FireEntityInit,
    flags: u32,
) -> *mut FireEntity {
    unsafe {
        type Ctor = unsafe extern "stdcall" fn(
            *mut FireEntity,
            *mut u8,
            *const FireEntityInit,
            u32,
        ) -> *mut FireEntity;
        let ctor: Ctor = core::mem::transmute(crate::rebase::rb(va::FIRE_ENTITY_CTOR));
        ctor(this, parent, init, flags)
    }
}
