use crate::class_type::ClassType;
use crate::fixed::Fixed;

/// A 32-bit pointer as it appears in WA's memory (the game is 32-bit x86).
pub type Ptr32 = u32;

/// Base task class in WA's entity hierarchy.
///
/// All game objects inherit from CTask. Tasks form a tree via parent/children
/// pointers and communicate through the TaskMessage system.
///
/// PARTIAL: Many fields between known offsets are unknown.
///
/// Source: wkJellyWorm CTask.h
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
    pub vtable: Ptr32,
    /// 0x04: Parent task in the hierarchy
    pub parent: Ptr32,
    /// 0x08: Unknown (set to 0x10 in constructor — possibly capacity/flags)
    pub _unknown_08: u32,
    /// 0x0C-0x1F: Children task list (CList<CTask*>)
    /// Internal structure: max_size(4), unk4(4), size(4), data_ptr(4), hash_list(4)
    pub _children_raw: [u8; 20],
    /// 0x20: Task classification type
    pub class_type: ClassType,
    /// 0x24-0x2F: Unknown padding to 0x30
    pub _unknown_24: [u8; 12],
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
    pub vtable2: Ptr32,
}

const _: () = assert!(core::mem::size_of::<CGameTask>() == 0xEC);
