use crate::fixed::Fixed;

use windows::Win32::Media::Audio::DirectSound::IDirectSoundBuffer;

use crate::audio::sound_id::SoundId;

/// Volume-to-dB attenuation table (64 entries of i16).
/// Copied from WA.exe .rdata at 0x6A6A60.
/// Index 0 = silence (-10000 dB), index 63 = near-unity (-22 dB).
/// Used by set_master_volume, set_channel_volume, and play_sound.
const VOLUME_DB_TABLE: [i16; 64] = [
    -10000, -6000, -5000, -4415, -4000, -3678, -3415, -3000, -2744, -2522, -2326, -2150, -1991,
    -1845, -1712, -1589, -1475, -1368, -1268, -1176, -1088, -1006, -928, -855, -786, -720, -658,
    -599, -543, -490, -439, -390, -344, -299, -256, -216, -177, -139, -104, -70, -38, -7, 24, 54,
    83, 111, 138, 163, 188, 211, 234, 256, 277, 296, 315, 333, 350, 367, 383, 398, 413, 427, 441,
    454,
]; // NOT: these might be negative centibels, not actual dB. Exact interpretation TBD.

/// DSSound — DirectSound audio subsystem.
///
/// Created by DSSound__Constructor (0x573D50).
/// Vtable: 0x66AF20 (24 slots).
/// Size: 0xBE0 bytes.
///
/// Manages sound playback through DirectSound.
/// Has 8 channel descriptors, 500 sound channel slots, a 64-entry
/// buffer pool, and a master volume control.

type Ptr32 = u32;

/// Channel descriptor (0x18 bytes). 8 entries at DSSound+0x14.
///
/// Each descriptor tracks one active DirectSound buffer being played.
/// Fields identified from vtable methods (update_channels, set_volume, etc.).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ChannelDescriptor {
    /// +0x00: The SoundId that was used to start this channel.
    /// Stored by core_play_sound; checked by set_master_volume/set_channel_volume
    /// for the raw-volume flag (bit 17).
    pub slot: SoundId,
    /// +0x04: Buffer pool index (0..63), or -1 if not pooled.
    /// Set by play_sound_pooled; checked by update_channels/stop_channel/allocate_channel.
    pub pool_idx: i32,
    /// +0x08: Sound priority (init -1, set to -1 on release).
    /// Used by allocate_channel for eviction: lowest priority channel is evicted first.
    pub priority: i32,
    /// +0x0C: Per-channel frequency (Fixed 16.16). Multiplied by status_2/status_1
    /// to compute actual playback frequency. Cleared to 0 on release.
    pub channel_freq: Fixed,
    /// +0x10: Per-channel volume (Fixed 16.16, 0..0x10000). Set by set_channel_volume.
    pub channel_volume: Fixed,
    /// +0x14: IDirectSoundBuffer* for the active buffer (0 = empty).
    /// At absolute offset this+0x28 for desc[0]. Use `buffer()` to get a typed ref.
    pub ds_buffer: Ptr32,
}

const _: () = assert!(core::mem::size_of::<ChannelDescriptor>() == 0x18);

impl ChannelDescriptor {
    /// Get the DirectSound buffer as a typed reference, if present.
    pub unsafe fn buffer(&self) -> Option<&IDirectSoundBuffer> {
        if self.ds_buffer != 0 {
            Some(&*(&self.ds_buffer as *const Ptr32 as *const IDirectSoundBuffer))
        } else {
            None
        }
    }

    /// Take ownership of the buffer (for releasing). Zeroes the field.
    pub unsafe fn take_buffer(&mut self) -> Option<IDirectSoundBuffer> {
        if self.ds_buffer != 0 {
            let buf: IDirectSoundBuffer = core::mem::transmute_copy(&self.ds_buffer);
            self.ds_buffer = 0;
            Some(buf)
        } else {
            None
        }
    }
}

/// DSSound vtable layout (24 slots at 0x66AF20).
///
/// Methods operate on the 8 channel descriptors and 64-entry buffer pool.
/// Trivial slots (stubs, noops, return-constant) are marked.
#[openwa_game::vtable(size = 24, va = 0x0066_AF20, class = "DSSound")]
pub struct DSSoundVtable {
    /// destructor
    pub destructor: fn(this: *mut DSSound, flags: u8) -> *mut DSSound,
    /// update_channels — iterates 8 descs, releases finished buffers
    pub update_channels: fn(this: *mut DSSound),
    /// set_frequency_scale — sets pitch scaling factors, adjusts active channel frequencies
    pub set_frequency_scale: fn(this: *mut DSSound, divisor: u32, multiplier: i32),
    /// play_sound — wrapper around core play. RET 0x14 = 5 stack params.
    pub play_sound: fn(
        this: *mut DSSound,
        slot: SoundId,
        priority: i32,
        volume: Fixed,
        pan: Fixed,
        freq: Fixed,
    ) -> bool,
    /// play_sound_pooled — allocates from buffer pool, plays. RET 0x14.
    pub play_sound_pooled: fn(
        this: *mut DSSound,
        slot: SoundId,
        priority: i32,
        volume: Fixed,
        pan: Fixed,
        freq: Fixed,
    ) -> i32,
    /// set_pan — sets pan on channel (dB lookup)
    pub set_pan: fn(this: *mut DSSound, channel: u32, pan: Fixed) -> u32,
    /// **stub** — returns 0
    pub stub_6: fn(this: *mut DSSound) -> u32,
    /// set_master_volume — sets volume, adjusts all channels
    pub set_master_volume: fn(this: *mut DSSound, volume: Fixed) -> u32,
    /// set_channel_volume — volume on specific channel
    pub set_channel_volume: fn(this: *mut DSSound, channel: i32, volume: Fixed) -> u32,
    /// is_channel_finished — returns 1 if stopped, 0 if playing
    pub is_channel_finished: fn(this: *mut DSSound, channel: i32) -> u8,
    /// stop_channel — stops + releases buffer, returns to pool
    pub stop_channel: fn(this: *mut DSSound, channel: i32) -> u32,
    /// release_finished — releases finished buffers, returns count
    pub release_finished: fn(this: *mut DSSound) -> i32,
    /// load_wav — opens WAV file, parses RIFF, creates buffer
    pub load_wav: fn(this: *mut DSSound, slot: i32, path: *const u8) -> u32,
    /// is_slot_loaded — returns channel_slots[idx] != 0
    pub is_slot_loaded: fn(this: *mut DSSound, slot: i32) -> bool,
    /// sub_destructor — sets secondary vtable
    pub sub_destructor: fn(this: *mut DSSound, flags: u8) -> *mut DSSound,
    /// **noop**
    pub noop_15: fn(this: *mut DSSound),
    /// **noop**
    pub noop_16: fn(this: *mut DSSound),
    /// **returns_0**
    pub returns_0_17: fn(this: *mut DSSound) -> u32,
    /// **returns_0** (same as 17)
    pub returns_0_18: fn(this: *mut DSSound) -> u32,
    /// **stub** — returns 0
    pub stub_19: fn(this: *mut DSSound) -> u32,
    /// **stub** — returns 0
    pub stub_20: fn(this: *mut DSSound) -> u32,
    /// **returns_0**
    pub returns_0_21: fn(this: *mut DSSound) -> u32,
    /// **stub** — returns 0
    pub stub_22: fn(this: *mut DSSound) -> u32,
    /// **returns_1**
    pub returns_1_23: fn(this: *mut DSSound) -> u32,
}

#[repr(C)]
pub struct DSSound {
    /// 0x000: Vtable pointer (0x66AF20)
    pub vtable: *const DSSoundVtable,
    /// 0x004: HWND (set after construction)
    pub hwnd: Ptr32,
    /// 0x008: IDirectSound* (from DirectSoundCreate)
    pub direct_sound: Ptr32,
    /// 0x00C: Primary buffer caps/format output from init_buffers
    pub primary_buffer_caps: u32,
    /// 0x010: IDirectSoundBuffer* (primary buffer)
    pub primary_buffer: Ptr32,
    /// 0x014-0xD3: 8 channel descriptors (0x18 bytes each = 0xC0 total)
    pub channel_descs: [ChannelDescriptor; 8],
    /// 0x0D4-0x8A3: 500 channel slot indices (zeroed by constructor).
    /// Each slot holds a pointer to an IDirectSoundBuffer for that sound effect.
    /// Index is 1-based (slot 0 unused). slot[i] != 0 means sound i is loaded.
    pub channel_slots: [u32; 500],
    /// 0x8A4: Master volume (Fixed, init 0x10000 = 1.0)
    pub volume: Fixed,
    /// 0x8A8-0x9A7: 64 buffer pool shadow entries (init all -1 by FUN_00574260).
    /// Maps pool index → channel descriptor index (-1 = free).
    pub buffer_pool_shadow: [i32; 64],
    /// 0x9A8-0xAA7: 64 buffer pool indices (init 0..63 by FUN_00574260).
    /// Free-list stack of available pool indices.
    pub buffer_pool_free: [u32; 64],
    /// 0xAA8: Number of free entries in buffer_pool_free (init 0x40 = 64).
    pub buffer_pool_free_count: u32,
    /// 0xAAC-0xBAB: Used pool tracking (0x100 bytes = 64 u32 entries).
    /// Tracks which pool indices are in use.
    pub buffer_pool_used: [u32; 64],
    /// 0xBAC: Number of used entries in buffer_pool_used (init 0).
    pub buffer_pool_used_count: u32,
    /// 0xBB0: Total bytes loaded (incremented by load_wav)
    pub total_bytes_loaded: u32,
    /// 0xBB4: Frequency scale divisor (init 1, used by set_frequency_scale)
    pub freq_scale_divisor: u32,
    /// 0xBB8: Frequency scale multiplier (init 1, used by set_frequency_scale)
    pub freq_scale_multiplier: u32,
    /// 0xBBC: Init success flag — set to 1 when DirectSoundCreate +
    /// init_buffers + IDirectSoundBuffer::Play all succeed.
    pub init_success: u32,
    /// 0xBC0-0xBDF: Unknown trailing fields
    pub _unknown_bc0: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<DSSound>() == 0xBE0);

bind_DSSoundVtable!(DSSound, vtable);

// ── Trivial vtable method implementations ─────────────────────────────────

// ── Rust vtable method implementations ────────────────────────────────────

/// Slot 13: is_slot_loaded — returns whether sound at `slot_idx` has a buffer loaded.
/// Slot index is used directly as array index (1-based, slot 0 unused).
pub unsafe extern "thiscall" fn is_slot_loaded(this: *mut DSSound, slot_idx: i32) -> bool {
    (*this)
        .channel_slots
        .get(slot_idx as usize)
        .copied()
        .unwrap_or(0)
        != 0
}

/// Trivial noop — returns void. Used for slots 15, 16.
pub unsafe extern "thiscall" fn noop(_this: *mut DSSound) {}

/// Trivial stub — returns 0. Used for slots 6, 17, 18, 19, 20, 21, 22.
pub unsafe extern "thiscall" fn returns_0(_this: *mut DSSound) -> u32 {
    0
}

/// Trivial stub — returns 1. Used for slot 23.
pub unsafe extern "thiscall" fn returns_1(_this: *mut DSSound) -> u32 {
    1
}

/// Slot 1: update_channels — iterates 8 channel descriptors, releases finished buffers.
/// Called each frame to clean up buffers that have finished playing.
pub unsafe extern "thiscall" fn update_channels(this: *mut DSSound) {
    let snd = &mut *this;
    for desc in &mut snd.channel_descs {
        if let Some(buf) = desc.buffer() {
            if let Ok(status) = buf.GetStatus() {
                // If not playing (bit 0 clear) and not pooled (pool_idx < 0), release.
                if (status & 1) == 0 && desc.pool_idx < 0 {
                    // Take and drop to release COM ref.
                    desc.take_buffer();
                    desc.channel_freq = Fixed(0);
                    desc.channel_volume = Fixed(0);
                    desc.priority = -1;
                    desc.pool_idx = -1;
                }
            }
        }
    }
}

/// Slot 11: release_finished — like update_channels but returns count of released buffers.
pub unsafe extern "thiscall" fn release_finished(this: *mut DSSound) -> i32 {
    let snd = &mut *this;
    let mut count = 0i32;
    for desc in &mut snd.channel_descs {
        if let Some(buf) = desc.buffer() {
            if let Ok(status) = buf.GetStatus() {
                if (status & 1) == 0 && desc.pool_idx < 0 {
                    desc.take_buffer();
                    desc.channel_freq = Fixed(0);
                    desc.channel_volume = Fixed(0);
                    desc.priority = -1;
                    desc.pool_idx = -1;
                    count += 1;
                }
            }
        }
    }
    count
}

/// Convert a fixed-point volume (0..0x10000) to a dB attenuation value
/// using the volume lookup table. Matches the original's arithmetic:
/// `(volume >> 16) << 6 >> 16` → index into VOLUME_DB_TABLE.
fn volume_to_db(volume: i32) -> i32 {
    let idx = ((volume as u32) >> 10) as usize; // (vol >> 16) << 6 = vol >> 10
    let clamped = idx.min(63);
    VOLUME_DB_TABLE[clamped] as i32
}

/// Slot 7: set_master_volume — sets the master volume and adjusts all active channels.
/// `new_volume` is Fixed 16.16 (0 = silent, 0x10000 = max).
pub unsafe extern "thiscall" fn set_master_volume(this: *mut DSSound, new_volume: Fixed) -> u32 {
    let snd = &mut *this;

    // Clamp to [0, 0x10000].
    let vol = new_volume.0.max(0).min(0x10000);

    // No change → early return.
    if snd.volume.0 == vol {
        return 1;
    }
    snd.volume = Fixed(vol);

    // Update all active channels.
    for desc in &snd.channel_descs {
        if desc.ds_buffer == 0 {
            continue;
        }
        // Skip channels using raw volume (not scaled by master).
        if desc.slot.is_raw_volume() {
            continue;
        }
        // Compute combined volume: master * per-channel, fixed-point multiply.
        let combined = ((vol as i64 * desc.channel_volume.0 as i64) >> 16) as i32;
        let db = volume_to_db(combined);
        if let Some(buf) = desc.buffer() {
            let _ = buf.SetVolume(db);
        }
    }

    1
}

/// Slot 2: set_frequency_scale — sets pitch scaling factors and adjusts active channel frequencies.
/// `divisor` is the frequency divisor, `multiplier` is the frequency multiplier.
/// For each playing channel: new_freq = channel_freq * multiplier / divisor, clamped to 200000.
pub unsafe extern "thiscall" fn set_frequency_scale(
    this: *mut DSSound,
    divisor: u32,
    multiplier: i32,
) {
    let snd = &mut *this;

    // No change → early return.
    if snd.freq_scale_divisor == divisor && snd.freq_scale_multiplier == multiplier as u32 {
        return;
    }
    snd.freq_scale_divisor = divisor;
    snd.freq_scale_multiplier = multiplier as u32;

    for desc in &snd.channel_descs {
        let Some(buf) = desc.buffer() else { continue };

        // Check if buffer is playing.
        let Ok(status) = buf.GetStatus() else {
            continue;
        };
        if status & 1 == 0 {
            continue;
        }

        // Compute adjusted frequency: channel_freq * multiplier / divisor.
        if divisor == 0 {
            continue;
        }
        let freq = (desc.channel_freq.0 as i64 * multiplier as i64 / divisor as i64) as i32;
        let freq = freq.min(200_000);

        let _ = buf.SetFrequency(freq as u32);
    }
}

/// Slot 5: set_pan — sets stereo panning on a specific channel.
/// `pool_id` is 1-based (1..64). `pan` is Fixed 16.16 (-0x10000..0x10000).
/// Negative = left, positive = right.
pub unsafe extern "thiscall" fn set_pan(this: *mut DSSound, pool_id: i32, pan: Fixed) -> u32 {
    let idx = (pool_id - 1) as usize;
    if idx >= 64 {
        return 0;
    }
    let snd = &*this;
    let desc_idx = snd.buffer_pool_shadow[idx];
    if desc_idx < 0 || desc_idx as usize >= 8 {
        return 0;
    }

    // Clamp pan to [-0x10000, 0x10000].
    let pan = pan.0.max(-0x10000).min(0x10000);

    // Index into table backwards: abs(pan) >> 10 gives 0..63.
    // Table is read from the END (index 63 = near silence, index 0 = full).
    let table_idx = ((pan.unsigned_abs() >> 10) as usize).min(63);
    let mut db = VOLUME_DB_TABLE[63 - table_idx] as i32;

    // Positive pan → negate (right channel louder = negative pan dB for left).
    if pan > 0 {
        db = -db;
    }

    if let Some(buf) = snd.channel_descs[desc_idx as usize].buffer() {
        let _ = buf.SetPan(db);
    }

    1
}

/// Slot 8: set_channel_volume — sets volume on a specific channel.
/// `pool_id` is 1-based (1..64). `volume` is Fixed 16.16 (0..0x10000).
pub unsafe extern "thiscall" fn set_channel_volume(
    this: *mut DSSound,
    pool_id: i32,
    volume: Fixed,
) -> u32 {
    let idx = (pool_id - 1) as usize;
    if idx >= 64 {
        return 0;
    }
    let snd = &mut *this;
    let desc_idx = snd.buffer_pool_shadow[idx];
    if desc_idx < 0 || desc_idx as usize >= 8 {
        return 0;
    }
    let di = desc_idx as usize;

    // Clamp volume to [0, 0x10000].
    let vol = volume.0.max(0).min(0x10000);

    // Compute dB: if flag 0x20000 set, use volume directly (no master scaling).
    let db = if snd.channel_descs[di].slot.is_raw_volume() {
        volume_to_db(vol)
    } else {
        let combined = ((snd.volume.0 as i64 * vol as i64) >> 16) as i32;
        volume_to_db(combined)
    };

    if let Some(buf) = snd.channel_descs[di].buffer() {
        let _ = buf.SetVolume(db);
    }

    // Store per-channel volume.
    snd.channel_descs[di].channel_volume = Fixed(vol);

    1
}

/// Slot 0: destructor — releases all COM objects and frees memory.
pub unsafe extern "thiscall" fn destructor(this: *mut DSSound, flags: u8) -> *mut DSSound {
    let snd = &mut *this;

    // Reset vtable to primary (destructor chain pattern).
    use crate::address::va;
    use crate::rebase::rb;
    snd.vtable = rb(va::DS_SOUND_VTABLE) as *const DSSoundVtable;

    // Release all 8 channel descriptor buffers (Stop + Release).
    for desc in &mut snd.channel_descs {
        if let Some(buf) = desc.take_buffer() {
            let _ = buf.Stop();
            // Release on drop
        }
    }

    // Release all 500 channel slot buffers.
    for slot in &mut snd.channel_slots {
        if *slot != 0 {
            let buf: IDirectSoundBuffer = core::mem::transmute_copy(slot);
            // Release on drop (no Stop needed — these are template buffers)
            drop(buf);
            *slot = 0;
        }
    }

    // Release primary buffer (Stop + Release).
    if snd.primary_buffer != 0 {
        let buf: IDirectSoundBuffer = core::mem::transmute_copy(&snd.primary_buffer);
        let _ = buf.Stop();
        drop(buf);
        snd.primary_buffer = 0;
    }

    // Release IDirectSound.
    if snd.direct_sound != 0 {
        use windows::Win32::Media::Audio::DirectSound::IDirectSound;
        let ds: IDirectSound = core::mem::transmute_copy(&snd.direct_sound);
        drop(ds);
        snd.direct_sound = 0;
    }

    // Set secondary vtable (base class destructor pattern).
    snd.vtable = rb(0x0066_AF58) as *const DSSoundVtable;

    if flags & 1 != 0 {
        crate::wa_alloc::wa_free(this as *mut u8);
    }
    this
}

/// Core play sound logic (replaces FUN_00574500).
/// Returns channel descriptor index on success, -1 on failure.
unsafe fn core_play_sound(
    snd: &mut DSSound,
    slot: SoundId,
    priority: i32,
    frequency: Fixed,
    volume: Fixed,
    pan: Fixed,
) -> i32 {
    use windows::Win32::Media::Audio::DirectSound::IDirectSound;

    // Check master volume and DirectSound initialized.
    if snd.volume.0 == 0 || snd.direct_sound == 0 {
        return -1;
    }

    // Validate slot index and check buffer loaded.
    let slot_idx = slot.index();
    if slot_idx == 0 || slot_idx > 499 || snd.channel_slots[slot_idx] == 0 {
        return -1;
    }

    // Allocate a channel descriptor (find free or evict lowest priority).
    let desc_idx = allocate_channel(snd, priority);
    if desc_idx < 0 {
        return -1;
    }
    let di = desc_idx as usize;

    // Get the template buffer from channel_slots.
    let template_buf: &IDirectSoundBuffer =
        &*(&snd.channel_slots[slot_idx] as *const Ptr32 as *const IDirectSoundBuffer);

    // Duplicate the buffer via IDirectSound::DuplicateSoundBuffer.
    let ds: &IDirectSound = &*(&snd.direct_sound as *const Ptr32 as *const IDirectSound);
    let new_buf = match ds.DuplicateSoundBuffer(template_buf) {
        Ok(buf) => buf,
        Err(_) => {
            snd.channel_descs[di].ds_buffer = 0;
            return -1;
        }
    };

    // Clamp volume to [0, 0x10000].
    let vol = volume.0.max(0).min(0x10000);

    // Compute per-channel volume: vol * 3/4 (with rounding).
    let vol_scaled = (vol * 3 + (((vol * 3) >> 31) & 3)) >> 2;

    // Compute dB for volume.
    let vol_db = if slot.is_raw_volume() {
        volume_to_db(vol_scaled)
    } else {
        let combined = ((snd.volume.0 as i64 * vol_scaled as i64) >> 16) as i32;
        volume_to_db(combined)
    };

    // Compute channel_freq: frequency * base_freq >> 16 (fixed-point multiply).
    let base_freq = template_buf.GetFrequency().unwrap_or(22050) as i32;
    let channel_freq = ((frequency.0.max(0) as i64 * base_freq as i64) >> 16) as i32;

    // Store descriptor state.
    let desc = &mut snd.channel_descs[di];
    desc.slot = slot;
    desc.pool_idx = -1;
    desc.priority = priority;
    desc.channel_freq = Fixed(channel_freq);
    desc.channel_volume = Fixed(vol_scaled);
    desc.ds_buffer = core::mem::transmute_copy(&new_buf);
    core::mem::forget(new_buf);

    // Compute actual playback frequency: channel_freq * multiplier / divisor.
    let freq = if snd.freq_scale_divisor != 0 {
        let f = (channel_freq as i64 * snd.freq_scale_multiplier as i64
            / snd.freq_scale_divisor as i64) as i32;
        f.min(200_000)
    } else {
        channel_freq
    };

    // Apply volume, frequency, pan to the duplicated buffer and start playback.
    if let Some(buf) = desc.buffer() {
        let _ = buf.SetCurrentPosition(0);
        let _ = buf.SetFrequency(freq as u32);
        let _ = buf.SetVolume(vol_db);

        // Pan: convert fixed-point to dB via reversed lookup table.
        let pan_clamped = pan.0.max(-0x10000).min(0x10000);
        let pan_idx = ((pan_clamped.unsigned_abs() >> 10) as usize).min(63);
        let mut pan_db = VOLUME_DB_TABLE[63 - pan_idx] as i32;
        if pan_clamped > 0 {
            pan_db = -pan_db;
        }
        let _ = buf.SetPan(pan_db);

        let _ = buf.Play(0, 0, if slot.is_looping() { 1 } else { 0 });
    }

    desc_idx
}

/// Channel allocator (replaces FUN_00574380).
/// Finds a free channel descriptor or evicts the lowest-priority one.
/// Returns descriptor index (0..7) or -1 if none available.
unsafe fn allocate_channel(snd: &mut DSSound, priority: i32) -> i32 {
    // First pass: find an empty descriptor.
    for i in 0..8 {
        if snd.channel_descs[i].ds_buffer == 0 {
            return i as i32;
        }
    }

    // All busy — find lowest priority channel to evict.
    let mut min_priority = i32::MAX;
    let mut min_idx: i32 = -1;
    for i in 0..8 {
        let p = snd.channel_descs[i].priority;
        if min_idx < 0 || p <= min_priority {
            min_priority = p;
            min_idx = i as i32;
        }
    }

    // Only evict if new sound has higher or equal priority.
    if min_idx >= 0 && min_priority <= priority {
        let di = min_idx as usize;
        let desc = &mut snd.channel_descs[di];

        // Clear pool shadow if this desc was in the pool.
        if desc.pool_idx >= 0 && (desc.pool_idx as usize) < 64 {
            snd.buffer_pool_shadow[desc.pool_idx as usize] = -1;
        }

        // Stop and release the evicted buffer.
        if let Some(buf) = desc.take_buffer() {
            let _ = buf.Stop();
        }

        return min_idx;
    }

    -1 // no channel available
}

/// Slot 3: play_sound — pure thiscall with 5 stack params, RET 0x14.
/// Params: slot (with flags), priority, frequency, volume, pan.
pub unsafe extern "thiscall" fn play_sound(
    this: *mut DSSound,
    slot: SoundId,
    priority: i32,
    frequency: Fixed,
    volume: Fixed,
    pan: Fixed,
) -> bool {
    let snd = &mut *this;
    let result = core_play_sound(snd, slot.without_loop(), priority, frequency, volume, pan);
    result >= 0
}

/// Slot 4: play_sound_pooled — pure thiscall with 5 stack params, RET 0x14.
/// Returns pool_id (1-based) on success, 0 on failure.
pub unsafe extern "thiscall" fn play_sound_pooled(
    this: *mut DSSound,
    slot: SoundId,
    priority: i32,
    frequency: Fixed,
    volume: Fixed,
    pan: Fixed,
) -> i32 {
    let snd = &mut *this;

    // Check pool has free entries.
    if snd.buffer_pool_free_count == 0 {
        return 0;
    }

    let desc_idx = core_play_sound(snd, slot, priority, frequency, volume, pan);
    if desc_idx < 0 {
        return 0;
    }

    // Pop a free pool index.
    snd.buffer_pool_free_count -= 1;
    let pool_idx = snd.buffer_pool_free[snd.buffer_pool_free_count as usize];

    // Add to used list.
    let used_slot = snd.buffer_pool_used_count as usize;
    snd.buffer_pool_used[used_slot] = pool_idx;
    snd.buffer_pool_used_count += 1;

    // Link pool ↔ descriptor.
    snd.buffer_pool_shadow[pool_idx as usize] = desc_idx;
    snd.channel_descs[desc_idx as usize].pool_idx = pool_idx as i32;

    (pool_idx + 1) as i32 // 1-based
}

/// Slot 14: sub_destructor — sets secondary vtable (0x66AF58), optionally frees.
/// This is a base-class destructor called by the primary destructor (slot 0).
pub unsafe extern "thiscall" fn sub_destructor(this: *mut DSSound, flags: u8) -> *mut DSSound {
    use crate::rebase::rb;
    // Set secondary vtable (base class vtable for destructor chain).
    (*this).vtable = rb(0x0066_AF58) as *const DSSoundVtable;
    if flags & 1 != 0 {
        crate::wa_alloc::wa_free(this as *mut u8);
    }
    this
}

/// Slot 9: is_channel_finished — checks if a buffer pool entry has stopped playing.
/// `pool_id` is 1-based (1..64).
///
/// Returns:
/// - **0** if the buffer IS playing (channel busy, don't reuse)
/// - **1** if the buffer is NOT playing, or error, or no desc (channel free)
/// - **0** if pool_id is out of range
///
/// The original inverts DSBSTATUS_PLAYING (bit 0): `NOT status; AND 1`.
pub unsafe extern "thiscall" fn is_channel_finished(this: *mut DSSound, pool_id: i32) -> u8 {
    let idx = (pool_id - 1) as usize;
    if idx >= 64 {
        return 0;
    }
    let snd = &*this;
    let desc_idx = snd.buffer_pool_shadow[idx];
    if desc_idx < 0 || desc_idx as usize >= 8 {
        return 1; // no desc → finished/free
    }
    let desc = &snd.channel_descs[desc_idx as usize];
    let Some(buf) = desc.buffer() else { return 1 };
    match buf.GetStatus() {
        Ok(status) => {
            if status & 1 != 0 {
                0
            } else {
                1
            } // playing → 0, stopped → 1
        }
        Err(_) => 1, // error → treat as finished
    }
}

/// Slot 10: stop_channel — stops a buffer, releases it, and returns the pool entry.
/// `pool_id` is 1-based (1..64). Returns 1 on success, 0 on invalid.
pub unsafe extern "thiscall" fn stop_channel(this: *mut DSSound, pool_id: i32) -> u32 {
    let idx = (pool_id - 1) as usize;
    if idx >= 64 {
        return 0;
    }
    let snd = &mut *this;
    let desc_idx = snd.buffer_pool_shadow[idx];

    // If the descriptor is still valid, stop and release the buffer.
    // (desc_idx may be -1 if the channel was evicted by allocate_channel)
    if desc_idx >= 0 && (desc_idx as usize) < 8 {
        let di = desc_idx as usize;
        let desc = &mut snd.channel_descs[di];

        // Stop and release the buffer.
        if let Some(buf) = desc.take_buffer() {
            let _ = buf.Stop();
        }

        // Reset descriptor to free state.
        desc.channel_freq = Fixed(0);
        desc.channel_volume = Fixed(0);
        desc.priority = -1;
        desc.pool_idx = -1;
    }

    // Mark pool shadow as free.
    snd.buffer_pool_shadow[idx] = -1;

    // ALWAYS return pool index to free list, even if descriptor was already
    // evicted. WA does this unconditionally — without it, evicted pool entries
    // leak and pool_free_count drops to 0, preventing new pooled sounds.
    let free_slot = snd.buffer_pool_free_count as usize;
    snd.buffer_pool_free[free_slot] = idx as u32;
    snd.buffer_pool_free_count += 1;

    // Remove from used list (swap-remove with last entry, matching WA).
    // Returns 1 if found in used list, 0 if not.
    let used_count = snd.buffer_pool_used_count as usize;
    for i in 0..used_count {
        if snd.buffer_pool_used[i] == idx as u32 {
            snd.buffer_pool_used_count -= 1;
            snd.buffer_pool_used[i] = snd.buffer_pool_used[snd.buffer_pool_used_count as usize];
            return 1;
        }
    }
    0
}

/// Slot 12: load_wav — load a WAV file into a DirectSound secondary buffer
/// and store the buffer pointer at `channel_slots[slot_idx]`.
///
/// Original at 0x573FF0: opens file, parses RIFF/WAVE, creates secondary
/// buffer via IDirectSound::CreateSoundBuffer, locks + fills + unlocks.
///
/// This Rust version uses `hound` for WAV parsing and the `windows` crate
/// for DirectSound COM calls.
///
/// Returns 1 on success, 0 on failure (matching original).
pub unsafe extern "thiscall" fn load_wav(
    this: *mut DSSound,
    slot_idx: i32,
    path: *const u8,
) -> u32 {
    use windows::Win32::Media::Audio::DirectSound::{
        IDirectSound, IDirectSoundBuffer, DSBLOCK_ENTIREBUFFER, DSBUFFERDESC,
    };
    use windows::Win32::Media::Audio::{WAVEFORMATEX, WAVE_FORMAT_PCM};

    // Validate: need DirectSound, valid slot, not already loaded.
    // Slot index is used directly as array index (1-based, slot 0 unused).
    let snd = &mut *this;
    if snd.direct_sound == 0 {
        return 0;
    }
    let slot = slot_idx as usize;
    if slot == 0 || slot > 499 || snd.channel_slots.get(slot).copied().unwrap_or(1) != 0 {
        return 0;
    }

    // Read path as C string.
    let c_path = match core::ffi::CStr::from_ptr(path as *const i8).to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };

    // Parse WAV with hound.
    let reader = match hound::WavReader::open(c_path) {
        Ok(r) => r,
        Err(_) => return 0,
    };
    let spec = reader.spec();
    let sample_bytes = (spec.bits_per_sample / 8) as u32;
    let block_align = spec.channels as u16 * sample_bytes as u16;
    let avg_bytes_per_sec = spec.sample_rate * block_align as u32;
    let data_len = reader.duration() * block_align as u32;

    // Build WAVEFORMATEX for the secondary buffer.
    let wfx = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: spec.channels,
        nSamplesPerSec: spec.sample_rate,
        nAvgBytesPerSec: avg_bytes_per_sec,
        nBlockAlign: block_align,
        wBitsPerSample: spec.bits_per_sample,
        cbSize: 0,
    };

    // DSBUFFERDESC flags: 0xE8 = DSBCAPS_CTRLVOLUME | DSBCAPS_CTRLPAN |
    //                            DSBCAPS_CTRLFREQUENCY | DSBCAPS_STATIC
    let desc = DSBUFFERDESC {
        dwSize: core::mem::size_of::<DSBUFFERDESC>() as u32,
        dwFlags: 0xE8,
        dwBufferBytes: data_len,
        dwReserved: 0,
        lpwfxFormat: &wfx as *const _ as *mut _,
        ..core::mem::zeroed()
    };

    // Create secondary buffer.
    let ds: &IDirectSound = &*(&snd.direct_sound as *const Ptr32 as *const IDirectSound);
    let mut buf: Option<IDirectSoundBuffer> = None;
    if ds.CreateSoundBuffer(&desc, &mut buf, None).is_err() {
        return 0;
    }
    let buf = match buf {
        Some(b) => b,
        None => return 0,
    };

    // Lock the buffer and fill with PCM data.
    let mut audio_ptr1: *mut core::ffi::c_void = core::ptr::null_mut();
    let mut audio_len1: u32 = 0;
    if buf
        .Lock(
            0,
            data_len,
            &mut audio_ptr1,
            &mut audio_len1,
            None,
            None,
            DSBLOCK_ENTIREBUFFER,
        )
        .is_err()
    {
        return 0;
    }

    // Read raw PCM bytes from the WAV file (hound gives us the data region).
    // We re-open to get raw bytes since hound's reader decodes samples.
    // Instead, use std::fs to read the raw data portion.
    // Actually, hound's into_inner() gives the underlying reader after headers.
    // But the simplest approach: just read the raw bytes from the data chunk.
    {
        let dest = core::slice::from_raw_parts_mut(audio_ptr1 as *mut u8, audio_len1 as usize);
        // hound exposes samples; for raw PCM we can collect them.
        // For 16-bit: each sample is i16. For 8-bit: u8.
        match spec.bits_per_sample {
            16 => {
                let mut reader = match hound::WavReader::open(c_path) {
                    Ok(r) => r,
                    Err(_) => {
                        let _ = buf.Unlock(audio_ptr1, audio_len1, None, 0);
                        return 0;
                    }
                };
                let mut offset = 0usize;
                for s in reader.samples::<i16>().flatten() {
                    if offset + 2 <= dest.len() {
                        dest[offset..offset + 2].copy_from_slice(&s.to_le_bytes());
                        offset += 2;
                    }
                }
            }
            8 => {
                let mut reader = match hound::WavReader::open(c_path) {
                    Ok(r) => r,
                    Err(_) => {
                        let _ = buf.Unlock(audio_ptr1, audio_len1, None, 0);
                        return 0;
                    }
                };
                let mut offset = 0usize;
                for s in reader.samples::<i16>().flatten() {
                    if offset < dest.len() {
                        // 8-bit WAV is unsigned (0-255, center at 128)
                        dest[offset] = (s + 128) as u8;
                        offset += 1;
                    }
                }
            }
            _ => {
                // Unsupported format — zero-fill
                dest.fill(0);
            }
        }
    }

    let _ = buf.Unlock(audio_ptr1, audio_len1, None, 0);

    // Store buffer pointer in channel_slots and track bytes loaded.
    snd.channel_slots[slot] = core::mem::transmute_copy(&buf);
    core::mem::forget(buf); // WA owns the COM reference
    snd.total_bytes_loaded += data_len;

    1 // success (original returns 1 on success, 0 on failure)
}

// ── Construction ──────────────────────────────────────────────────────────

impl DSSound {
    /// Construct a DSSound with all fields initialized, matching the
    /// original constructor (0x573D50) + helpers FUN_005742A0/FUN_00574260.
    ///
    /// Uses WA's vtable pointer for identity (same pattern as DisplayBase).
    ///
    /// # Safety
    /// Must be called from within the WA.exe process (needs rebased vtable).
    pub unsafe fn new(hwnd: u32) -> Self {
        use crate::address::va;
        use crate::rebase::rb;

        let mut snd: Self = core::mem::zeroed();

        // Vtable (WA's .rdata, identity-checked)
        snd.vtable = rb(va::DS_SOUND_VTABLE) as *const DSSoundVtable;

        // HWND
        snd.hwnd = hwnd;

        // Volume = 1.0 (16.16 fixed-point)
        snd.volume = Fixed(0x10000);

        // 8 channel descriptors: pool_idx=-1, priority=-1
        // (rest is zero from mem::zeroed)
        for desc in &mut snd.channel_descs {
            desc.pool_idx = -1;
            desc.priority = -1;
        }

        // 500 channel slots: already zeroed

        // Buffer pool: shadow all -1, free list 0..63
        for (i, slot) in snd.buffer_pool_free.iter_mut().enumerate() {
            *slot = i as u32;
        }
        for entry in &mut snd.buffer_pool_shadow {
            *entry = -1;
        }
        snd.buffer_pool_free_count = 64;

        // Frequency scaling (1:1 = no scaling)
        snd.freq_scale_divisor = 1;
        snd.freq_scale_multiplier = 1;
        // init_success stays 0 until COM init succeeds

        snd
    }
}
