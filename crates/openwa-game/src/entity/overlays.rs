use super::game_entity::WorldEntity;
use crate::FieldRegistry;

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
