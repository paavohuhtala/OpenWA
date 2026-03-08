/// DDGameWrapper — large wrapper around DDGame.
///
/// Created by DDGameWrapper__Constructor (0x56DEF0).
/// Holds the DDGame pointer, graphics handlers, landscape, and display state.
///
/// Vtable: 0x66A30C
///
/// Note: Ghidra shows DWORD-indexed offsets (param_2[0x122] etc.).
/// Byte offset = dword_index * 4.
///
/// The constructor accesses at least up to DWORD index 0x1BBA (byte offset 0x6EE8),
/// making this object at least ~28KB. Total size is not yet determined.
///
/// PARTIAL: Only confirmed fields are defined. The repr(C) struct uses
/// a conservative size that covers known fields. The actual object is larger.
#[repr(C)]
pub struct DDGameWrapper {
    /// 0x000: Vtable pointer (0x66A30C)
    pub vtable: *mut u8,
    /// 0x004-0x487: Unknown fields
    pub _unknown_004: [u8; 0x484],
    /// 0x488: Pointer to DDGame allocation (DWORD index 0x122)
    pub ddgame: *mut u8,
    /// 0x48C: Secondary DDGame struct pointer (0x2C bytes, conditional)
    pub ddgame_secondary: *mut u8,
    /// 0x490-0x4BF: Unknown
    pub _unknown_490: [u8; 0x30],
    /// 0x4C0: Unknown object pointer (not GfxHandler — vtable reads as 0)
    pub _field_4c0: *mut u8,
    /// 0x4C4: Unknown pointer
    pub _field_4c4: *mut u8,
    /// 0x4C8: Graphics mode flag (DWORD index 0x132)
    pub gfx_mode: u32,
    /// 0x4CC: PCLandscape object pointer (DWORD index 0x133)
    pub landscape: *mut u8,
    /// 0x4D0: DDDisplay/display object pointer (param2 of constructor)
    pub display: *mut u8,
    /// 0x4D4: DSSound/sound object pointer (param3 of constructor)
    pub sound: *mut u8,
    /// 0x4D8: Init 0 (DWORD index 0x136)
    pub _field_4d8: u32,
    /// 0x4DC: Calculated value (DWORD index 0x137)
    pub _field_4dc: u32,
    /// 0x4E0: Init -100 / 0xFFFFFF9C (DWORD index 0x138)
    pub _field_4e0: u32,
    /// 0x4E4-0x6EEB: Unknown fields (extends to at least DWORD 0x1BBA)
    pub _unknown_4e4: [u8; 0x6A08],
    /// 0x6EEC: Init 0 (DWORD index 0x1BBA)
    pub _field_6eec: u32,
    /// 0x6EF0-end: Unknown trailing fields
    pub _unknown_6ef0: [u8; 0x10],
}

// This is a minimum estimate. The actual object is likely larger.
const _: () = assert!(core::mem::size_of::<DDGameWrapper>() == 0x6F00);
