use crate::audio::dssound::DSSound;
use crate::display::dd_display::DDDisplay;
use crate::engine::ddgame::DDGame;
use crate::render::landscape::PCLandscape;

/// Speech name table entry size (0x40 = 64 bytes, null-terminated C string).
pub const SPEECH_NAME_ENTRY_SIZE: usize = 0x40;
/// Maximum number of speech name entries.
pub const SPEECH_NAME_TABLE_LEN: usize = 360;

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
/// PARTIAL: Only confirmed fields are defined.
#[repr(C)]
pub struct DDGameWrapper {
    /// 0x000: Vtable pointer (0x66A30C)
    pub vtable: *mut u8,
    /// 0x004-0x487: Unknown fields
    pub _unknown_004: [u8; 0x484],
    /// 0x488: Pointer to DDGame allocation (DWORD index 0x122)
    pub ddgame: *mut DDGame,
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
    pub landscape: *mut PCLandscape,
    /// 0x4D0: DDDisplay pointer (param2 of constructor)
    pub display: *mut DDDisplay,
    /// 0x4D4: DSSound pointer (param3 of constructor)
    pub sound: *mut DSSound,
    /// 0x4D8: Loading progress counter (incremented per loading tick).
    pub loading_progress: u32,
    /// 0x4DC: Loading progress total (base 0x2AD + 0x38 per team + 0x7E overhead).
    pub loading_total: u32,
    /// 0x4E0: Last displayed loading percentage (init -100 to force first update).
    pub loading_last_pct: u32,
    /// 0x4E4-0x14E7: Unknown fields
    pub _unknown_4e4: [u8; 0x14E8 - 0x4E4],
    /// 0x14E8: Speech name table — 360 entries of 0x40-byte C strings.
    /// Used by DDGameWrapper__LoadSpeechWAV to deduplicate loaded WAVs.
    pub speech_name_table: [[u8; SPEECH_NAME_ENTRY_SIZE]; SPEECH_NAME_TABLE_LEN],
    /// 0x6EE8: Number of entries used in speech_name_table.
    pub speech_name_count: u32,
    /// 0x6EEC: Init 0 (DWORD index 0x1BBA)
    pub _field_6eec: u32,
    /// 0x6EF0-end: Unknown trailing fields
    pub _unknown_6ef0: [u8; 0x10],
}

const _: () = assert!(core::mem::size_of::<DDGameWrapper>() == 0x6F00);
