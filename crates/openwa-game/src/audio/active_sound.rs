use crate::engine::ddgame::DDGame;
use crate::task::SoundEmitter;
use openwa_core::fixed::Fixed;

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
    /// 0x600: Running counter — incremented on each insert, masked to index (& 0x3F).
    pub counter: u32,
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
    /// 0x10: Running counter value at time of insertion (unique ID).
    pub sequence: i32,
    /// 0x14: DSSound channel handle (return from play_sound_pooled). 0 = free slot.
    pub channel_handle: u32,
}

const _: () = assert!(core::mem::size_of::<ActiveSoundEntry>() == 0x18);

impl ActiveSoundTable {
    /// Stop an active streaming sound by handle. Port of FUN_00546490.
    ///
    /// The `handle` should have the 0x40000000 bit already cleared (the caller
    /// strips it before calling). The low 6 bits index into the entry table.
    ///
    /// Returns true if the sound was found and stopped.
    pub unsafe fn stop_sound(&mut self, handle: i32) -> bool {
        let slot = (handle & 0x3F) as usize;
        let entry = &mut self.entries[slot];

        if entry.sequence != handle || entry.channel_handle == 0 {
            return false;
        }

        // Stop the DSSound channel via the DDGame's sound system
        let ddgame = &*self.ddgame;
        let sound = ddgame.sound;
        if !sound.is_null() {
            ((*(*sound).vtable).stop_channel)(sound, entry.channel_handle as i32);
        }
        entry.channel_handle = 0;

        // Release the emitter reference
        if !entry.emitter.is_null() {
            (*(entry.emitter as *mut SoundEmitter)).local_ref_count -= 1;
            entry.emitter = core::ptr::null_mut();
        }

        true
    }
}
