use crate::engine::ddgame::DDGame;
use crate::fixed::Fixed;

/// Active sound tracking table — manages positional (local) sound playback.
///
/// Allocated conditionally in DDGame__Constructor when sound is available
/// (`game_info+0xF914 == 0` and `DSSound != NULL`), right after
/// `DSSound_LoadEffectWAVs` and `DSSound_LoadAllSpeechBanks`.
///
/// Stored at DDGame+0x00C. Tracks up to 64 active positional sounds
/// with world coordinates and volume for 3D audio mixing.
///
/// Layout: 64 entries (0x18 bytes each) + count + DDGame back-pointer.
#[repr(C)]
pub struct ActiveSoundTable {
    /// 0x000-0x5FF: 64 sound entries (entry 0 is unused/reserved).
    pub entries: [ActiveSoundEntry; 64],
    /// 0x600: Number of active entries (max 64).
    pub count: u32,
    /// 0x604: Back-pointer to owning DDGame.
    pub ddgame: *mut DDGame,
}

const _: () = assert!(core::mem::size_of::<ActiveSoundTable>() == 0x608);

/// A single entry in the active sound table (0x18 = 24 bytes).
///
/// Tracks a positional sound currently playing through DSSound.
/// Entries with `emitter == NULL` may still have valid position/volume
/// from a recently finished sound.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ActiveSoundEntry {
    /// 0x00: Pointer to the emitting task (or NULL if emitter finished).
    pub emitter: *mut u8,
    /// 0x04: World X position (fixed-point 16.16).
    pub pos_x: Fixed,
    /// 0x08: World Y position (fixed-point 16.16).
    pub pos_y: Fixed,
    /// 0x0C: Volume (fixed-point 16.16, 0x10000 = 1.0).
    pub volume: Fixed,
    /// 0x10: DSSound channel/slot index (1-based).
    pub channel_index: u32,
    /// 0x14: Flags (0x40 observed on actively emitting entry).
    pub flags: u32,
}

const _: () = assert!(core::mem::size_of::<ActiveSoundEntry>() == 0x18);
