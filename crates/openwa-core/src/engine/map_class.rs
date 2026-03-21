/// MapClass — landscape/terrain map object (0x29628 bytes).
///
/// Used by the replay loader to parse map data from playback.thm files.
/// Allocated with CRT malloc (0x29628 bytes), constructed via
/// `MapClass__Constructor` (0x447E80), loaded via `MapClass__Load`
/// (0x44A9A0), then map info is copied to game state via
/// `MapClass__CopyInfo` (0x449B60, usercall ESI=this).
///
/// After use, released via vtable[1] (thiscall destructor with free flag).
///
/// PARTIAL: Only the vtable and terrain_flag field are known.
#[repr(C)]
pub struct MapClass {
    /// 0x00000: Vtable pointer.
    pub vtable: *const MapClassVtable,
    /// 0x00004-0x29617: Unknown (map data, terrain grid, etc.).
    pub _unknown_004: [u8; 0x29618 - 4],
    /// 0x29618: Terrain type flag. Zero = cavern terrain (SETZ in replay loader).
    pub terrain_flag: u8,
    /// 0x29619-0x29627: Unknown trailing bytes.
    pub _unknown_29619: [u8; 0x29628 - 0x29619],
}

const _: () = assert!(core::mem::size_of::<MapClass>() == 0x29628);

/// MapClass vtable (partial — only known slots).
#[repr(C)]
pub struct MapClassVtable {
    /// Slot 0: Unknown virtual method.
    pub slot_0: usize,
    /// Slot 1: Destructor — thiscall(this, free_flag). Frees the object if flag & 1.
    pub destructor: unsafe extern "thiscall" fn(*mut MapClass, i32),
}
