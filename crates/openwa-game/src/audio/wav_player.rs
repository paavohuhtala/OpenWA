//! WavPlayer — standalone WAV playback via DirectSound.
//!
//! WA.exe uses WavPlayer instances for frontend SFX, fanfares, and music
//! playback. Each instance is an 8-byte struct at a fixed global address.
//! This module provides pure Rust replacements for the three core operations:
//!
//! - `WavPlayer_Stop` (0x599670): Release the loaded DirectSound buffer
//! - `WavPlayer_Play` (0x5996E0): Set volume and start playback
//! - `WavPlayer_LoadAndPlay` (0x599B40): Parse WAV, create DS buffer, store
//!
//! The WavPlayer system uses its own IDirectSound instance (global at
//! 0x79D654), separate from DSSound's instance.

use std::ffi::{c_char, c_void, CStr, CString};

use windows::Win32::Media::Audio::DirectSound::{
    IDirectSound, IDirectSoundBuffer, DSBLOCK_ENTIREBUFFER, DSBUFFERDESC,
};
use windows::Win32::Media::Audio::{WAVEFORMATEX, WAVE_FORMAT_PCM};

use crate::rebase::rb;

// ============================================================
// Address constants
// ============================================================

crate::define_addresses! {
    /// IDirectSound* for the WavPlayer subsystem (separate from DSSound).
    /// Null before DirectSound init.
    global G_WAV_DIRECT_SOUND = 0x0079_D654;
    /// Master volume for WavPlayer playback (0-100).
    global G_WAV_MASTER_VOLUME = 0x0069_7704;
    /// Success sentinel: a result pointer equal to this address means "no error".
    global G_WAV_RESULT_SUCCESS = 0x008A_C8A0;
}

/// Volume-to-dB attenuation table (65 entries of i16, indices 0-64).
/// Copied from WA.exe .rdata at 0x697680.
/// Index 0 = silence (-10000 cB), index 64 = unity (0 cB).
/// Used by WavPlayer_Play to convert the 0-100 master volume to DirectSound
/// hundredths-of-decibels via: `table[(vol * 64) / 100]`.
const WAV_VOLUME_DB_TABLE: [i16; 65] = [
    -10000, -6000, -5000, -4415, -4000, -3678, -3415, -3000, -2744, -2522, -2326, -2150, -1991,
    -1845, -1712, -1589, -1475, -1368, -1268, -1176, -1088, -1006, -928, -855, -786, -720, -658,
    -599, -543, -490, -439, -390, -344, -299, -256, -216, -177, -139, -104, -70, -38, -7, 24, 54,
    83, 111, 138, 163, 188, 211, 234, 256, 277, 296, 315, 333, 350, 367, 383, 398, 413, 427, 441,
    454, 0,
];

// ============================================================
// WavPlayer struct layout
// ============================================================

/// WavPlayer instance (8 bytes at a fixed global address).
///
/// Layout matches WA.exe:
/// - `+0x00`: Pointer to static vtable/type data (0x651CC8)
/// - `+0x04`: Pointer to WavBuffer (null = nothing loaded)
#[repr(C)]
pub struct WavPlayer {
    pub type_ptr: u32,
    pub buffer: *mut WavBuffer,
}

/// Loaded WAV buffer (heap-allocated, owned by the WavPlayer system).
///
/// The first two fields must match WA's layout because unhooked WA code
/// (FUN_005999c0) reads `filename` directly from buffer+4.
#[repr(C)]
pub struct WavBuffer {
    /// Raw IDirectSoundBuffer COM pointer (stored as u32 for FFI).
    pub ds_buffer: u32,
    /// Null-terminated filename (C string, allocated via CString::into_raw).
    pub filename: *mut c_char,
}

// ============================================================
// Helpers
// ============================================================

/// Read the WavPlayer IDirectSound* global. Returns 0 if not initialized.
#[inline]
unsafe fn wav_direct_sound_ptr() -> u32 {
    *(rb(G_WAV_DIRECT_SOUND) as *const u32)
}

/// Read the WavPlayer master volume (0-100).
#[inline]
unsafe fn wav_master_volume() -> u32 {
    *(rb(G_WAV_MASTER_VOLUME) as *const u32)
}

/// Read the success sentinel value.
#[inline]
pub unsafe fn wav_result_success() -> u32 {
    *(rb(G_WAV_RESULT_SUCCESS) as *const u32)
}

/// Reinterpret a `*const u32` (pointing to a stored raw COM pointer) as
/// `&IDirectSoundBuffer`. The pointer must point to stable memory (e.g. a
/// struct field), NOT a stack local — the returned reference borrows it.
#[inline]
unsafe fn ds_buffer_from_field(field_ptr: *const u32) -> &'static IDirectSoundBuffer {
    &*(field_ptr as *const IDirectSoundBuffer)
}

// ============================================================
// WavPlayer_Stop — port of 0x599670
// ============================================================

/// Stop and release the loaded DirectSound buffer.
///
/// If no buffer is loaded (player.buffer == null), this is a no-op.
/// Otherwise: releases the DirectSound buffer COM object, frees the
/// filename string, and deallocates the WavBuffer struct.
pub unsafe fn wav_player_stop(player: *mut WavPlayer) {
    let buf = (*player).buffer;
    if buf.is_null() {
        return;
    }

    // Free filename string (was allocated via CString::into_raw)
    let filename = (*buf).filename;
    if !filename.is_null() {
        drop(CString::from_raw(filename));
    }

    // Stop and release DirectSound buffer
    let ds_ptr = (*buf).ds_buffer;
    if wav_direct_sound_ptr() != 0 && ds_ptr != 0 {
        let ds_buf: IDirectSoundBuffer = core::mem::transmute_copy(&ds_ptr);
        let _ = ds_buf.Stop();
        // Drop calls Release()
    }

    // Free the WavBuffer struct (was allocated via Box::into_raw)
    drop(Box::from_raw(buf));
    (*player).buffer = core::ptr::null_mut();
}

// ============================================================
// WavPlayer_Play — port of 0x5996E0
// ============================================================

/// Rewind, set volume, and start playback on the loaded buffer.
///
/// The `flags` parameter maps to IDirectSoundBuffer::Play dwFlags
/// (0 = play once, DSBPLAY_LOOPING = loop). The volume is read from
/// the global master volume at 0x697704 and converted via the dB table.
pub unsafe fn wav_player_play(player: *mut WavPlayer, flags: u32) {
    let ds_raw = wav_direct_sound_ptr();
    if ds_raw == 0 {
        return;
    }

    let buf = (*player).buffer;
    if buf.is_null() {
        return;
    }

    if (*buf).ds_buffer == 0 {
        return;
    }

    // Reference the COM pointer from the heap-allocated WavBuffer field
    // (NOT from a stack copy — that would create a dangling reference).
    let ds_buf = ds_buffer_from_field(&(*buf).ds_buffer);

    // Rewind to start (port of FUN_00599930's SetCurrentPosition call)
    let _ = ds_buf.SetCurrentPosition(0);

    // Compute volume: (master_vol * 64) / 100, then look up in dB table
    let master_vol = wav_master_volume();
    let idx = ((master_vol << 6) / 100).min(64) as usize;
    let vol_db = WAV_VOLUME_DB_TABLE[idx] as i32;
    let _ = ds_buf.SetVolume(vol_db);

    // Play
    let _ = ds_buf.Play(0, 0, flags);
}

// ============================================================
// WavPlayer_LoadAndPlay — port of 0x599B40
// ============================================================

/// Parse a WAV file, create a DirectSound buffer, and store it in the player.
///
/// Uses `hound` for WAV parsing and the `windows` crate for DirectSound.
/// Only PCM format is supported (WA.exe rejects non-PCM too).
///
/// `param3` controls DSBCAPS_GLOBALFOCUS: if >= 0, the flag is set.
/// All known callers pass 0.
///
/// Returns true on success, false on failure.
pub unsafe fn wav_player_load_and_play(
    player: *mut WavPlayer,
    path: *const c_char,
    param3: i32,
) -> bool {
    // Need DirectSound
    let ds_raw = wav_direct_sound_ptr();
    if ds_raw == 0 {
        return false;
    }

    // Stop and free current buffer
    wav_player_stop(player);

    // Open WAV file
    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };
    let reader = match hound::WavReader::open(path_str) {
        Ok(r) => r,
        Err(_) => return false,
    };
    let spec = reader.spec();

    // Only support PCM
    if spec.sample_format != hound::SampleFormat::Int {
        return false;
    }

    // Compute format parameters
    let block_align = spec.channels * (spec.bits_per_sample / 8);
    let avg_bytes_per_sec = spec.sample_rate * block_align as u32;
    let data_len = reader.duration() * block_align as u32;

    if data_len == 0 {
        return false;
    }

    // Build WAVEFORMATEX
    let wfx = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: spec.channels,
        nSamplesPerSec: spec.sample_rate,
        nAvgBytesPerSec: avg_bytes_per_sec,
        nBlockAlign: block_align,
        wBitsPerSample: spec.bits_per_sample,
        cbSize: 0,
    };

    // DSBUFFERDESC flags: DSBCAPS_STATIC (0x40) | DSBCAPS_CTRLVOLUME (0x80)
    // Plus DSBCAPS_GLOBALFOCUS (0x8000) if param3 >= 0
    let mut flags: u32 = 0xC0; // STATIC | CTRLVOLUME
    if param3 >= 0 {
        flags |= 0x8000; // GLOBALFOCUS
    }

    let desc = DSBUFFERDESC {
        dwSize: core::mem::size_of::<DSBUFFERDESC>() as u32,
        dwFlags: flags,
        dwBufferBytes: data_len,
        dwReserved: 0,
        lpwfxFormat: &wfx as *const _ as *mut _,
        ..Default::default()
    };

    // Create secondary buffer
    let ds: &IDirectSound = &*(&ds_raw as *const u32 as *const IDirectSound);
    let mut buf_opt: Option<IDirectSoundBuffer> = None;
    if ds.CreateSoundBuffer(&desc, &mut buf_opt, None).is_err() {
        return false;
    }
    let ds_buf = match buf_opt {
        Some(b) => b,
        None => return false,
    };

    // Lock the entire buffer
    let mut audio_ptr: *mut c_void = core::ptr::null_mut();
    let mut audio_len: u32 = 0;
    if ds_buf
        .Lock(
            0,
            data_len,
            &mut audio_ptr,
            &mut audio_len,
            None,
            None,
            DSBLOCK_ENTIREBUFFER,
        )
        .is_err()
    {
        return false;
    }

    // Fill with PCM data from the WAV file
    {
        let dest = core::slice::from_raw_parts_mut(audio_ptr as *mut u8, audio_len as usize);
        match spec.bits_per_sample {
            16 => {
                // Re-open to get fresh sample iterator
                if let Ok(mut reader) = hound::WavReader::open(path_str) {
                    let mut offset = 0usize;
                    for s in reader.samples::<i16>().flatten() {
                        if offset + 2 <= dest.len() {
                            dest[offset..offset + 2].copy_from_slice(&s.to_le_bytes());
                            offset += 2;
                        }
                    }
                }
            }
            8 => {
                if let Ok(mut reader) = hound::WavReader::open(path_str) {
                    let mut offset = 0usize;
                    for s in reader.samples::<i16>().flatten() {
                        if offset < dest.len() {
                            dest[offset] = (s + 128) as u8;
                            offset += 1;
                        }
                    }
                }
            }
            _ => {
                dest.fill(0);
            }
        }
    }

    let _ = ds_buf.Unlock(audio_ptr, audio_len, None, 0);

    // Store filename (CString for proper deallocation in wav_player_stop)
    let filename = match CString::new(path_str) {
        Ok(cs) => cs.into_raw(),
        Err(_) => core::ptr::null_mut(),
    };

    // Store DS buffer as raw COM pointer (we take ownership)
    let ds_raw_ptr: u32 = core::mem::transmute_copy(&ds_buf);
    core::mem::forget(ds_buf); // Don't Release — stored in WavBuffer

    // Allocate WavBuffer and store in player
    let wav_buf = Box::new(WavBuffer {
        ds_buffer: ds_raw_ptr,
        filename,
    });
    (*player).buffer = Box::into_raw(wav_buf);

    true
}
