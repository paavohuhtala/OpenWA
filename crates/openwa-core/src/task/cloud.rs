use super::base::CTask;
use crate::fixed::Fixed;
use crate::FieldRegistry;

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
#[derive(FieldRegistry)]
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
