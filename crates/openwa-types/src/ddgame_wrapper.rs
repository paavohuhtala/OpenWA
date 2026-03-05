use crate::task::Ptr32;

/// DDGameWrapper — thin wrapper around DDGame.
///
/// This is the top-level object created by DDGameWrapper__Constructor (0x56DEF0).
/// It holds a pointer to the DDGame allocation and graphics handler references.
///
/// Vtable: 0x66A30C
///
/// Note: Ghidra shows DWORD-indexed offsets (param_1[0x122] etc.).
/// Byte offset = dword_index * 4.
///
/// PARTIAL: Only confirmed fields from the constructor are defined.
#[repr(C)]
pub struct DDGameWrapper {
    /// 0x000: Vtable pointer (0x66A30C)
    pub vtable: Ptr32,
    /// 0x004-0x487: Unknown fields
    pub _unknown_004: [u8; 0x484],
    /// 0x488: Pointer to DDGame allocation
    pub ddgame: Ptr32,
    /// 0x48C: Secondary DDGame struct pointer (0x2C bytes, conditional)
    pub ddgame_secondary: Ptr32,
    /// 0x490-0x4BF: Unknown
    pub _unknown_490: [u8; 0x30],
    /// 0x4C0: Graphics handler 0 pointer (0x19C bytes, vtable 0x66B280)
    pub gfx_handler_0: Ptr32,
    /// 0x4C4: Graphics handler 1 pointer (optional)
    pub gfx_handler_1: Ptr32,
    /// 0x4C8: Graphics mode flag
    pub gfx_mode: u32,
    /// 0x4CC: PCLandscape object pointer
    pub landscape: Ptr32,
    /// 0x4D0: Constructor param_2
    pub _param_4d0: u32,
    /// 0x4D4: Constructor param_3
    pub _param_4d4: u32,
    /// 0x4D8-end: Unknown trailing fields
    /// Size uncertain — DDGameWrapper total size not yet determined
    pub _unknown_4d8: [u8; 0x28],
}

// DDGameWrapper total size is uncertain. This is a minimum based on known fields.
// Remove or update this assertion when the true size is determined.
const _: () = assert!(core::mem::size_of::<DDGameWrapper>() == 0x500);
