use crate::task::Ptr32;

/// DDGame — the main game engine object.
///
/// This is a massive ~39KB struct (0x98B8 bytes) that owns all major subsystems:
/// display, landscape, sound, graphics handlers, and task state machines.
///
/// Allocated in DDGame__Constructor (0x56E220).
/// Vtable set via DDGameWrapper at 0x66A30C.
///
/// PARTIAL: Only landmark offsets are defined. The vast majority of fields
/// are unknown. Use the offset constants below for field access.
///
/// Note: Ghidra decompiles DDGame constructor with DWORD-indexed offsets
/// (param_2[0x122] etc.), but these are byte offsets in the struct below.
/// Conversion: dword_index * 4 = byte_offset.
#[repr(C)]
pub struct DDGame {
    /// 0x0000-0x0023: Header region
    pub _header: [u8; 0x24],
    /// 0x0024: Game state pointer
    pub game_state: Ptr32,
    /// 0x0028: Unknown parameter from constructor
    pub _param_28: Ptr32,
    /// 0x002C-0x11AF: Unknown fields
    pub _unknown_002c: [u8; 0x1184],
    /// 0x11B0-0x11BF: Task objects region (5 state machine pointers)
    /// Ghidra: param[0x46C..0x47C] (dword-indexed)
    pub task_ptrs: [Ptr32; 5],
    /// 0x11C4-0x1217: Unknown
    pub _unknown_11c4: [u8; 0x54],
    /// 0x1218-0x1223: Game global / WormKit-known offsets area
    /// Note: WormKit documents DDGame+0x488 as game_global, but that's
    /// relative to the DDGame ptr held in DDGameWrapper[0x122].
    /// Since DDGameWrapper[0x122] points to THIS struct at offset 0,
    /// these WormKit offsets map directly.
    pub _unknown_1218: [u8; 0x270],
    /// 0x1488-0x3547: Unknown fields
    pub _unknown_1488: [u8; 0x20C0],
    /// 0x3548: Display mode pointer
    pub display_mode: Ptr32,
    /// 0x354C: Display width
    pub display_width: u32,
    /// 0x3550: Display param (init 0)
    pub _display_3550: u32,
    /// 0x3554: Display param (init 0)
    pub _display_3554: u32,
    /// 0x3558-0x355F: Unknown
    pub _unknown_3558: [u8; 8],
    /// 0x3560: Display center X (width / 2)
    pub display_center_x: u32,
    /// 0x3564: Display center Y (height / 2)
    pub display_center_y: u32,
    /// 0x3568-0x3577: Unknown
    pub _unknown_3568: [u8; 0x10],
    /// 0x3578: Window handle (HWND)
    pub hwnd: Ptr32,
    /// 0x357C-0x358C: Unknown
    pub _unknown_357c: [u8; 0x11],
    /// 0x358D-0x398C: Palette entries (0x100 * 4 = 1024 bytes)
    pub palette: [u8; 0x400],
    /// 0x398D-0x3D8F: Unknown display fields
    pub _unknown_398d: [u8; 0x403],
    /// 0x3D90: Graphics constant (init 0x100)
    pub _gfx_3d90: u32,
    /// 0x3D94: Graphics constant (init 0xFFFFFFFF)
    pub _gfx_3d94: u32,
    /// 0x3D98: Graphics object pointers (4 entries)
    pub gfx_objects: [Ptr32; 4],
    /// 0x3DA8-0x98B7: Remaining fields
    pub _unknown_3da8: [u8; 0x5B10],
}

const _: () = assert!(core::mem::size_of::<DDGame>() == 0x98B8);

/// Well-known byte offsets into DDGame, for use with raw pointer access.
///
/// These are sourced from WormKit, wkJellyWorm, and Ghidra decompilation.
/// WormKit documents offsets relative to DDGameWrapper[0x122] which points
/// to the DDGame allocation.
pub mod offsets {
    /// Game state pointer
    pub const GAME_STATE: usize = 0x0024;

    /// Task state machine pointers (5 entries, each Ptr32)
    pub const TASK_PTRS: usize = 0x11B0;

    /// Display mode pointer
    pub const DISPLAY_MODE: usize = 0x3548;
    /// Display width
    pub const DISPLAY_WIDTH: usize = 0x354C;
    /// Display center X
    pub const DISPLAY_CENTER_X: usize = 0x3560;
    /// Display center Y
    pub const DISPLAY_CENTER_Y: usize = 0x3564;
    /// Window handle (HWND)
    pub const HWND: usize = 0x3578;
    /// Palette start (0x400 bytes)
    pub const PALETTE: usize = 0x358D;

    /// Graphics object pointers (from DDDisplay__Init)
    pub const GFX_OBJECTS: usize = 0x3D98;
}
