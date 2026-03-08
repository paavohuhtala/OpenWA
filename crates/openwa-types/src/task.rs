use crate::class_type::ClassType;
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
    pub ddgame: *mut u8,
}

const _: () = assert!(core::mem::size_of::<CTask>() == 0x30);

/// Game task - extends CTask with physics and gameplay data.
///
/// PARTIAL: Most fields between 0x30-0x83 and 0x98-0xEC are unknown.
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
    /// 0xE8: Secondary vtable pointer (-> 0x669CF8 for CGameTask)
    pub vtable2: *mut u8,
}

const _: () = assert!(core::mem::size_of::<CGameTask>() == 0xEC);
