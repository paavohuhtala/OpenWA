use super::base::BaseEntity;
use super::game_entity::{SoundEmitter, WorldEntity};
use crate::FieldRegistry;
use openwa_core::fixed::Fixed;

// TODO: There is no such thing as bungee trail - so what is this?
/// Bungee trail rendering entity fields.
///
/// Used by DrawBungeeTrail (0x500720). Fields at 0xBC-0xE4 overlap with
/// WorldEntity's `_unknown_98` region — different entity types may use these
/// offsets for different purposes.
///
/// Cast a entity pointer to this type when you know it's a bungee trail entity.
#[derive(FieldRegistry)]
#[repr(C)]
pub struct BungeeTrailEntity {
    /// 0x00-0x2F: BaseEntity base
    pub base: BaseEntity,
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

const _: () = assert!(core::mem::size_of::<BungeeTrailEntity>() == 0xFC);

/// Weapon aiming entity fields.
///
/// Used by DrawCrosshairLine (0x5197D0). Fields at 0x258+ are in the derived
/// class region beyond WorldEntity (0xFC). The exact class name is unknown.
///
/// Cast a entity pointer to this type when you know it's a worm/weapon aiming entity.
#[derive(FieldRegistry)]
#[repr(C)]
pub struct WeaponAimEntity {
    /// 0x00-0xFB: WorldEntity base
    pub game_entity: WorldEntity,
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
    /// 0x324: Aim range offset (added to GameWorld crosshair scale)
    pub aim_range_offset: i32,
}
