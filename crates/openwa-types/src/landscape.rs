use crate::task::Ptr32;

/// PCLandscape — terrain/landscape subsystem.
///
/// Created by PCLandscape__Constructor (0x57ACB0).
/// Vtable: 0x66B208
///
/// Manages terrain data, water effects, and level graphics.
/// Loads Water.dir and Level.dir from `data\Gfx\`.
/// Allocates a 384KB (0x60000) terrain buffer.
///
/// PARTIAL: Only constructor-confirmed fields are defined.
/// Ghidra uses DWORD-indexed offsets (param_1[0x240] etc.).
/// Byte offset = dword_index * 4.
#[repr(C)]
pub struct PCLandscape {
    /// 0x000: Vtable pointer (0x66B208)
    pub vtable: Ptr32,
    /// 0x004: Parent DDGame pointer
    pub ddgame: Ptr32,
    /// 0x008-0x0C7: Unknown
    pub _unknown_008: [u8; 0xC0],
    /// 0x0C8: Water effect object pointer (0xBC bytes, vtable 0x66B268)
    pub water_effect: Ptr32,
    /// 0x0CC-0x8E7: Unknown fields (terrain metadata, dimensions, etc.)
    pub _unknown_0cc: [u8; 0x81C],
    /// 0x8E8: Terrain buffer pointer (0x60000 bytes allocated)
    pub terrain_buffer: Ptr32,
    /// 0x8EC-0x8EF: Buffer size (0x60000)
    pub _terrain_size: [u8; 4],
    /// 0x8F0-0x90F: Unknown
    pub _unknown_8f0: [u8; 0x20],
    /// 0x910-0x913: Terrain handler pointer
    pub terrain_handler: Ptr32,
    /// 0x914: Initialized flag (set to 1)
    pub initialized: u32,
    /// 0x918-0x92F: Terrain directory path pointers
    pub _dir_paths: [u8; 0x18],
    /// 0x930: Splash buffer pointer (0x4C bytes)
    pub splash_buffer: Ptr32,
    /// 0x934: Terrain data pointer
    pub terrain_data: Ptr32,
    /// 0x938: Palette pointer
    pub palette: Ptr32,
    /// 0x93C: Shader object pointer (vtable 0x66B1DC)
    pub shader: Ptr32,
    /// 0x940-0x947: Surface texture buffer pointers
    pub surface_textures: [Ptr32; 2],
    /// 0x948-0x94B: Unknown
    pub _unknown_948: [u8; 4],
    /// 0x94C-0xB33: Unknown
    pub _unknown_94c: [u8; 0x1E8],
    /// 0xB34: Water .dir file handle
    pub water_dir_handle: Ptr32,
    /// 0xB38: Level .dir file handle
    pub level_dir_handle: Ptr32,
    /// 0xB3C-0xB3F: Trailing padding
    pub _unknown_b3c: [u8; 4],
}

const _: () = assert!(core::mem::size_of::<PCLandscape>() == 0xB40);
