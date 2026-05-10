// ============================================================
// Sound queue entry — 16 entries at GameWorld + 0x7F00
// ============================================================

use openwa_core::vec2::Vec2;

/// Sound queue entry (0x24 = 36 bytes, stride between consecutive entries).
///
/// GameWorld maintains a 16-slot sound queue at offset 0x7F00. PlaySoundGlobal
/// appends entries; PlaySoundLocal additionally marks entries as local and
/// stores the emitter sub-object pointer for position updates.
#[repr(C)]
pub struct SoundQueueEntry {
    /// Sound effect ID (SoundId enum value).
    pub sound_id: u32,
    /// Flags / priority (e.g. 3=weapon, 7=explosion).
    pub flags: u32,
    /// Volume (Fixed-point, 0x10000 = 1.0).
    pub volume: u32,
    /// Pitch (Fixed-point, 0x10000 = 1.0).
    pub pitch: u32,
    /// Reserved, always 0.
    pub reserved: u32,
    /// 0 = global, 1 = local (has position tracking).
    pub is_local: u8,
    pub _pad: [u8; 3],
    pub pos: Vec2,
    /// Pointer to the emitter sub-object (entity + 0xE8) for position updates.
    pub emitter: *const u32,
}

const _: () = assert!(core::mem::size_of::<SoundQueueEntry>() == 0x24);
