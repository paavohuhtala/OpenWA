use super::base::BaseEntity;
use super::game_task::WorldEntity;
use crate::FieldRegistry;

crate::define_addresses! {
    class "CrateEntity" {
        ctor CRATE_ENTITY_CTOR = 0x00502490;
    }
}

#[openwa_game::vtable(size = 12, va = 0x00664298, class = "CrateEntity")]
pub struct CrateEntityVtable {
    /// HandleMessage — processes crate messages (collection, parachute, etc.).
    /// thiscall + 4 stack params, RET 0x10.
    #[slot(2)]
    pub handle_message: fn(
        this: *mut CrateEntity,
        sender: *mut BaseEntity,
        msg_type: u32,
        size: u32,
        data: *const u8,
    ),
    /// ProcessFrame — per-frame crate update.
    /// thiscall + 1 stack param (flags), RET 0x4.
    #[slot(7)]
    pub process_frame: fn(this: *mut CrateEntity, flags: u32),
}

/// Weapon/health/utility crate entity task.
///
/// Extends WorldEntity (0xFC bytes). Crates fall from the sky with a parachute,
/// land on terrain, and are collected by worms on contact. Can contain weapons,
/// health, or utility items depending on crate type.
///
/// Constructor: 0x502490 (stdcall, 4 params: this, parent, scheme_data, spawn_mode).
/// Vtable: 0x664298. Class type byte: 0x0F.
///
/// The constructor copies 0xE5 DWORDs (0x394 bytes) of scheme/crate data from
/// param_3 into offset 0x110, making the crate carry its full configuration.
///
/// Source: Ghidra decompilation of 0x502490, wkJellyWorm CrateEntity.h
#[derive(FieldRegistry)]
#[repr(C)]
pub struct CrateEntity {
    /// 0x00–0xFB: WorldEntity base (pos at 0x84/0x88, speed at 0x90/0x94)
    pub base: WorldEntity<*const CrateEntityVtable>,
    /// 0xFC: Unknown (zeroed by constructor)
    pub _unknown_fc: u32,
    /// 0x100: This crate's slot ID in `GameWorld.entity_activity_queue`.
    pub activity_rank_slot: u32,
    /// 0x104: Unknown (zeroed by constructor)
    pub _unknown_104: u32,
    /// 0x108: Unknown (zeroed by constructor, also cleared for spawn_mode=1)
    pub _unknown_108: u32,
    /// 0x10C: Timer/counter (zeroed by constructor; set to scheme_data[0x52]*1000
    /// for crate_type == 3)
    pub timer: u32,
    /// 0x110–0x4A3: Scheme/crate data (0xE5 DWORDs = 0x394 bytes, copied from
    /// constructor param_3). Contains weapon properties, crate type, quantities, etc.
    ///
    /// Key indices (DWORD offsets from 0x110):
    ///   [0x05] (0x124): crate_type — discriminator (3=timed?, 5=airstrike?)
    ///   [0x4F] (0x24C): health crate healing amount
    ///   [0x45] (0x224): scheme weapon data sub-field
    ///   [0x55] (0x264): nonzero triggers additional init
    ///   [0x85] (0x324): additional scheme params
    pub scheme_data: [u32; 0xE5],
    /// 0x4A4: Unknown (zeroed by constructor)
    pub _unknown_4a4: u32,
    /// 0x4A8: Sequence/reference index (-1 = none; set conditionally from GameWorld+0x51C)
    pub sequence_ref: i32,
    /// 0x4AC: Unknown (zeroed by constructor as param_1[299])
    pub _unknown_4ac: u32,
}

const _: () = assert!(core::mem::size_of::<CrateEntity>() == 0x4B0);

// Generate typed vtable method wrappers: handle_message(), process_frame().
bind_CrateEntityVtable!(CrateEntity, base.base.vtable);
