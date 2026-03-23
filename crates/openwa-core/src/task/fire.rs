use super::base::CTask;
use crate::FieldRegistry;

crate::define_addresses! {
    class "CTaskFire" {
        /// CTaskFire vtable - fire/flame entity (0xD8 bytes)
        vtable CTASK_FIRE_VTABLE = 0x0066_9DD8;
        ctor CTASK_FIRE_CTOR = 0x0054_F4C0;
    }
}

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
#[derive(FieldRegistry)]
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
