/// Music — music playback object (0x354 bytes).
///
/// Combines a playlist controller with an embedded [`StreamingAudio`] engine.
/// Created by `GameEngine::InitHardware`, stored at `GameWorld+0x014`.
///
/// Constructor: 0x58BC10, usercall(ESI=this) + stack(IDirectSound*, path), RET 0x8.
/// Vtable: 0x66B3E0 (6 slots).
///
/// The music system maps track IDs 1-13 to WAV files under the base path
/// (typically `Data\streams`).
use std::ffi::c_char;

use openwa_core::fixed::Fixed;

/// Raw COM pointer stored as u32 (matches the 32-bit target's pointer size).
type Ptr32 = u32;

define_addresses! {
    class "Music" {
        ctor MUSIC_CONSTRUCTOR = 0x0058BC10;
    }
    /// Music destructor (called from scalar deleting dtor).
    fn MUSIC_DESTRUCTOR = 0x0058BC80;
    /// Music::PlayTrack internal — maps track ID to filename, calls StreamingAudio::Open.
    fn/Usercall MUSIC_PLAY_TRACK = 0x0058BD20;
    /// Music volume dB lookup table (65 entries, i32).
    data MUSIC_VOLUME_DB_TABLE = 0x006AF960;
    /// StreamingAudio::Init (zero-init base object).
    fn/Usercall STREAMING_AUDIO_INIT = 0x004707B0;
    /// StreamingAudio::Open — opens WAV, configures buffer, starts playback.
    fn/Usercall STREAMING_AUDIO_OPEN = 0x00574B30;
    /// StreamingAudio::Stop — kills timer, releases DS buffer, closes mmio.
    fn/Usercall STREAMING_AUDIO_STOP = 0x00574CA0;
    /// StreamingAudio::InitPlayback — core setup (timer caps, DS buffer, timer).
    fn/Stdcall STREAMING_AUDIO_INIT_PLAYBACK = 0x00574D10;
    /// StreamingAudio::OpenWAV — parse RIFF/WAVE via mmio.
    fn/Usercall STREAMING_AUDIO_OPEN_WAV = 0x00574F80;
    /// StreamingAudio::FillAndStart — initial buffer fill + DS Play + timeSetEvent.
    fn/Usercall STREAMING_AUDIO_FILL_AND_START = 0x005751B0;
    /// StreamingAudio::Reset — kill timer, stop buffer, seek back.
    fn/Usercall STREAMING_AUDIO_RESET = 0x005752D0;
    /// StreamingAudio::TimerCallback — periodic buffer refill (stdcall, RET 0x14).
    fn/Stdcall STREAMING_AUDIO_TIMER_CALLBACK = 0x005753A0;
    /// StreamingAudio::ReadChunk — read clamped WAV data from mmio.
    fn STREAMING_AUDIO_READ_CHUNK = 0x00575370;
}

/// Music object — playlist controller + embedded streaming engine.
///
/// Total size: 0x354 bytes (852 bytes).
#[repr(C)]
pub struct Music {
    /// 0x000: Vtable pointer (0x66B3E0, 6 slots).
    pub vtable: *const MusicVtable,
    /// 0x004: Playlist track IDs (max 32 entries).
    pub playlist: [u32; 32],
    /// 0x084: Base path for music files (e.g. "DATA\\Music"), 256 bytes.
    pub base_path: [c_char; 256],
    /// 0x184: Number of tracks in current playlist.
    pub playlist_count: u32,
    /// 0x188: Index of currently playing track in playlist.
    pub current_track_index: u32,
    /// 0x18C: IDirectSound instance (raw COM pointer).
    pub direct_sound: Ptr32,
    /// 0x190: Embedded streaming audio engine (0x1C4 bytes).
    pub streaming: StreamingAudio,
}

const _: () = assert!(core::mem::size_of::<Music>() == 0x354);

/// Streaming audio engine — WAV file streaming via DirectSound double-buffering.
///
/// Embedded at Music+0x190. The original WA code uses mmio API for WAV I/O;
/// our Rust port uses `hound` + `std::fs::File` for reading, and
/// `timeSetEvent` for periodic buffer refill.
///
/// Size: 0x1C4 bytes (452 bytes).
#[repr(C)]
pub struct StreamingAudio {
    /// 0x00: DirectSound ring buffer size in bytes.
    pub ds_buffer_size: u32,
    /// 0x04: Unknown / unused.
    pub _unknown_04: u32,
    /// 0x08: Buffer segment count (quality-dependent: 24/5/4/3 for 8k/16k/22k/44kHz).
    pub buffer_segments: u32,
    /// 0x0C: log2(disk sector size) — shift count for sector alignment.
    pub sector_shift: u8,
    /// 0x0D: Padding.
    pub _pad_0d: [u8; 3],
    /// 0x10: Timer interval in milliseconds (quality-dependent: 2000/600/400/100ms).
    pub timer_delay_ms: u32,
    /// 0x14: Timer resolution in milliseconds (timer_delay_ms / 4).
    pub timer_resolution_ms: u32,
    /// 0x18: Number of aligned sectors in ring buffer.
    pub num_sectors: u32,
    /// 0x1C: Ring buffer write cursor position (bytes).
    pub write_cursor: u32,
    /// 0x20: Total bytes played (accumulated from play cursor delta).
    pub bytes_played: u32,
    /// 0x24: Total bytes read from WAV data chunk.
    pub file_bytes_read: u32,
    /// 0x28: Previous play cursor position for delta calculation.
    pub last_play_cursor: u32,
    /// 0x2C: Flags bitfield.
    /// - bit 0: looping
    /// - bit 1: threaded open
    /// - bit 2: auto-start
    /// - bit 14 (0x4000): DS buffer created/active
    /// - bit 15 (0x8000): stopping
    pub flags: u32,
    /// 0x30: Completion callback (LPTHREAD_START_ROUTINE), called when non-looping playback ends.
    pub completion_callback: u32,
    /// 0x34: User data for completion callback.
    pub completion_callback_data: u32,
    /// 0x38: Timer capabilities (TIMECAPS: wPeriodMin, wPeriodMax).
    pub time_caps: [u32; 2],
    /// 0x40: Resolved timer period (clamped from TIMECAPS).
    pub timer_period: u32,
    /// 0x44: Timer ID from timeSetEvent.
    pub timer_id: u32,
    /// 0x48: Absolute path to WAV file (MAX_PATH = 260 bytes).
    pub file_path: [c_char; 260],
    /// 0x14C: WAV format (WAVEFORMATEX, 18 bytes for PCM).
    pub wave_format: WaveFormatPcm,
    /// 0x15E: Padding after WAVEFORMATEX.
    pub _pad_15e: [u8; 2],
    /// 0x160: mmio file handle — repurposed as Box<std::fs::File> in Rust.
    /// Original: HMMIO. We store a raw pointer to a heap-allocated File.
    /// 0 / null when no file is open.
    pub file_handle: u32,
    /// 0x164: Data chunk info (reused MMCKINFO in original; we store data_size + data_offset).
    pub data_chunk: MmckInfo,
    /// 0x178: Snapshot of bytes_played (copied each timer tick).
    pub bytes_played_snapshot: u32,
    /// 0x17C: 1 when DS buffer is live, 0 otherwise.
    pub is_initialized: u32,
    /// 0x180: Last HRESULT from DirectSound operations.
    pub last_error: u32,
    /// 0x184: Unknown / unused.
    pub _unknown_184: u32,
    /// 0x188: End-of-stream threshold in bytes (fade-out distance).
    pub fade_out_bytes: u32,
    /// 0x18C: Cumulative timer callback duration (timeGetTime).
    pub timer_time_total: u32,
    /// 0x190: Average timer callback time (timer_time_total / timer_call_count).
    pub avg_timer_time: u32,
    /// 0x194: Number of timer callback invocations.
    pub timer_call_count: u32,
    /// 0x198: DS buffer status from GetStatus.
    pub ds_buffer_status: u32,
    /// 0x19C: The DirectSound secondary buffer for playback (raw COM pointer).
    pub ds_buffer: Ptr32,
    /// 0x1A0: IDirectSound device reference (raw COM pointer).
    pub direct_sound: Ptr32,
    /// 0x1A4: Unknown trailing data.
    pub _unknown_1a4: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<StreamingAudio>() == 0x1C4);

/// WAVEFORMATEX PCM subset (18 bytes) — matches the Windows WAVEFORMATEX layout.
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct WaveFormatPcm {
    pub format_tag: u16,
    pub channels: u16,
    pub samples_per_sec: u32,
    pub avg_bytes_per_sec: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
    pub cb_size: u16,
}

const _: () = assert!(core::mem::size_of::<WaveFormatPcm>() == 18);

/// MMCKINFO (20 bytes) — multimedia chunk info.
/// We reuse the struct layout for compatibility but only care about ck_size (data length)
/// and data_offset (file offset of PCM data start).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MmckInfo {
    pub ck_id: u32,
    pub ck_size: u32,
    pub fcc_type: u32,
    pub data_offset: u32,
    pub dw_flags: u32,
}

const _: () = assert!(core::mem::size_of::<MmckInfo>() == 20);

/// Music vtable — 6 slots at 0x66B3E0.
///
/// Slot order verified by reading vtable data from binary:
///   +0x00: 0x58BC60  +0x04: 0x58BEE0  +0x08: 0x58BF40
///   +0x0C: 0x58BE70  +0x10: 0x58BE90  +0x14: 0x58BCE0
#[openwa_game::vtable(size = 6, va = 0x0066B3E0, class = "Music")]
pub struct MusicVtable {
    /// scalar_deleting_dtor(flags) — 0x58BC60
    pub scalar_deleting_dtor: fn(this: *mut Music, flags: u32),
    /// advance_track — 0x58BEE0
    pub advance_track: fn(this: *mut Music),
    /// set_volume(volume) — 0x58BF40. Volume is Fixed 16.16.
    pub set_volume: fn(this: *mut Music, volume: Fixed),
    /// start_music(track_id) — 0x58BE70
    pub start_music: fn(this: *mut Music, track_id: u32),
    /// set_playlist(tracks, count) — 0x58BE90
    pub set_playlist: fn(this: *mut Music, tracks: *const u32, count: u32),
    /// stop_and_cleanup — 0x58BCE0
    pub stop_and_cleanup: fn(this: *mut Music),
}

/// Track ID to WAV filename mapping (from PlayTrack switch at 0x58BD20).
/// Track IDs 1-13 map to these filenames.
pub const MUSIC_TRACKS: [&str; 13] = [
    "ingame-01-generic.wav",          // 1
    "ingame-02-cavern.wav",           // 2
    "ingame-03-jungle.wav",           // 3
    "ingame-04-battlezone.wav",       // 4
    "ingame-05-forest.wav",           // 5
    "ingame-06-weird-alien-plan.wav", // 6
    "ingame-07-outerspace.wav",       // 7
    "ingame-08-desert.wav",           // 8
    "ingame-09-hell.wav",             // 9
    "ingame-10-mech-workshop.wav",    // 10
    "ingame-11-rain&surf.wav",        // 11
    "suddendeath2-loop.wav",          // 12
    "suddendeath1-loop.wav",          // 13
];

/// Streaming audio flags.
pub mod streaming_flags {
    /// Sound should loop when reaching end of data.
    pub const LOOP: u32 = 0x0001;
    /// Open the file on a worker thread.
    pub const THREADED: u32 = 0x0002;
    /// Auto-start playback after opening.
    pub const AUTO_START: u32 = 0x0004;
    /// DirectSound buffer has been created and is active.
    pub const DS_BUFFER_ACTIVE: u32 = 0x4000;
    /// Playback is stopping / fading out.
    pub const STOPPING: u32 = 0x8000;
}

// ============================================================
// Music volume dB lookup table
// ============================================================

/// Volume-to-dB lookup table for music (65 entries).
/// Index: volume * 64 / 0x10000, maps Fixed 16.16 volume to centibels.
/// Copied from WA.exe .rdata at 0x6AF960.
/// Index 0 = -10000 cB (silence), index 64 = 0 cB (full volume).
const MUSIC_VOLUME_DB: [i32; 65] = [
    -10000, -6644, -5644, -5059, -4644, -4322, -4059, -3837, -3644, -3473, -3322, -3185, -3059,
    -2943, -2837, -2737, -2644, -2557, -2474, -2396, -2322, -2251, -2184, -2119, -2059, -2000,
    -1943, -1889, -1837, -1787, -1737, -1690, -1644, -1600, -1557, -1515, -1474, -1434, -1396,
    -1358, -1322, -1286, -1251, -1217, -1184, -1151, -1119, -1088, -1059, -1028, -1000, -971, -943,
    -915, -889, -862, -837, -811, -787, -762, -737, -714, -690, -667, 0,
];

// ============================================================
// StreamingAudio implementation
// ============================================================

use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicU32, Ordering};

use windows::Win32::Media::Audio::DirectSound::{DSBUFFERDESC, IDirectSound, IDirectSoundBuffer};
use windows::Win32::Media::Audio::{WAVE_FORMAT_PCM, WAVEFORMATEX};

/// Global pointer to the active StreamingAudio for the timer callback.
/// Only one streaming audio instance is active at a time (music playback).
static STREAMING_AUDIO_PTR: AtomicU32 = AtomicU32::new(0);

/// Reinterpret a `*const Ptr32` field (pointing to a stored raw COM pointer)
/// as `&IDirectSoundBuffer`. The pointer must point to stable memory (e.g. a
/// struct field), NOT a stack local — the returned reference borrows it.
#[inline]
unsafe fn ds_buf_from_field(field: *const Ptr32) -> &'static IDirectSoundBuffer {
    unsafe { &*(field as *const IDirectSoundBuffer) }
}

/// Reinterpret a `*const Ptr32` field (pointing to a stored raw COM pointer)
/// as `&IDirectSound`.
#[inline]
unsafe fn ds_from_field(field: *const Ptr32) -> &'static IDirectSound {
    unsafe { &*(field as *const IDirectSound) }
}

impl StreamingAudio {
    /// Zero-initialize the streaming audio engine. Port of FUN_004707B0.
    pub fn init(&mut self) {
        // Zero everything except keep the struct's memory layout intact
        unsafe {
            core::ptr::write_bytes(
                self as *mut Self as *mut u8,
                0,
                core::mem::size_of::<Self>(),
            );
        }
    }

    /// Open a WAV file and start streaming playback. Port of FUN_00574B30.
    ///
    /// `flags` is a combination of `streaming_flags` constants.
    /// `path` is the absolute path to the WAV file.
    /// `ds` is the IDirectSound instance for buffer creation.
    pub unsafe fn open(&mut self, path: &str, flags: u32, ds: Ptr32) -> bool {
        unsafe {
            // If already initialized, stop first
            if self.is_initialized != 0 {
                self.stop();
            }

            self.direct_sound = ds;
            self.flags = flags;
            self.completion_callback = 0;
            self.completion_callback_data = 0;

            // Determine quality parameters from flags (original switch on flags & 0x78).
            match flags & 0x78 {
                0x08 => {
                    // 8kHz quality
                    self.buffer_segments = 24;
                    self.timer_delay_ms = 2000;
                }
                0x10 => {
                    // 16kHz
                    self.buffer_segments = 5;
                    self.timer_delay_ms = 600;
                }
                0x20 => {
                    // 22kHz (default for music with flags 0x24/0x25)
                    self.buffer_segments = 4;
                    self.timer_delay_ms = 400;
                }
                0x40 => {
                    // 44kHz
                    self.buffer_segments = 3;
                    self.timer_delay_ms = 100;
                }
                _ => {
                    // Invalid quality — original returns E_INVALIDARG
                    return false;
                }
            }

            // Copy path to file_path buffer
            let path_bytes = path.as_bytes();
            let copy_len = path_bytes.len().min(259);
            core::ptr::copy_nonoverlapping(
                path_bytes.as_ptr(),
                self.file_path.as_mut_ptr() as *mut u8,
                copy_len,
            );
            self.file_path[copy_len] = 0; // null terminate

            // Initialize playback (open WAV, create buffer, start timer)
            self.init_playback()
        }
    }

    /// Core playback setup: open WAV file, create DirectSound buffer, start timer.
    /// Port of FUN_00574D10 + FUN_00574F80 + FUN_005751B0.
    unsafe fn init_playback(&mut self) -> bool {
        unsafe {
            // Open the WAV file using hound
            let path_cstr = std::ffi::CStr::from_ptr(self.file_path.as_ptr());
            let path_str = match path_cstr.to_str() {
                Ok(s) => s,
                Err(_) => {
                    let _ = openwa_core::log::log_line("[Music] init_playback: invalid UTF-8 path");
                    return false;
                }
            };

            let reader = match hound::WavReader::open(path_str) {
                Ok(r) => r,
                Err(e) => {
                    let _ = openwa_core::log::log_line(&format!(
                        "[Music] init_playback: failed to open \"{}\": {}",
                        path_str, e
                    ));
                    return false;
                }
            };

            let spec = reader.spec();

            // Only PCM format supported
            if spec.sample_format != hound::SampleFormat::Int {
                return false;
            }

            // Store format info
            let block_align = spec.channels as u16 * (spec.bits_per_sample as u16 / 8);
            let avg_bytes_per_sec = spec.sample_rate * block_align as u32;
            self.wave_format = WaveFormatPcm {
                format_tag: 1, // WAVE_FORMAT_PCM
                channels: spec.channels as u16,
                samples_per_sec: spec.sample_rate,
                avg_bytes_per_sec,
                block_align,
                bits_per_sample: spec.bits_per_sample as u16,
                cb_size: 0,
            };

            // Store data chunk info
            let data_len = reader.len() * (spec.bits_per_sample as u32 / 8);
            self.data_chunk.ck_size = data_len;

            // Get data start position from the inner reader, then extract the File.
            // After WavReader::new(), the inner reader is positioned at data start.
            let mut inner = reader.into_inner();
            let data_start = inner.stream_position().unwrap_or(0);
            self.data_chunk.data_offset = data_start as u32;

            // Store the BufReader<File> as a raw pointer for streaming reads
            let file = Box::new(inner);
            self.file_handle = Box::into_raw(file) as u32;

            // Compute buffer size matching original float arithmetic (constant at 0x679760 = 1000.0):
            //   per_tick = round(timer_delay_ms * avg_bytes_per_sec / 1000.0)
            //   ds_buffer_size = per_tick * buffer_segments
            let per_tick =
                (self.timer_delay_ms as f64 * avg_bytes_per_sec as f64 / 1000.0).round() as u32;
            let buffer_bytes = per_tick * self.buffer_segments;
            self.ds_buffer_size = buffer_bytes;

            // Timer resolution
            self.timer_resolution_ms = self.timer_delay_ms / 4;

            // Fade-out threshold
            let fade_ms = self.timer_delay_ms + self.timer_delay_ms / 4;
            self.fade_out_bytes = fade_ms * (avg_bytes_per_sec / 1000);

            // Create DirectSound secondary buffer (streaming, no STATIC flag)
            let wfx = WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_PCM as u16,
                nChannels: spec.channels as u16,
                nSamplesPerSec: spec.sample_rate,
                nAvgBytesPerSec: avg_bytes_per_sec,
                nBlockAlign: block_align,
                wBitsPerSample: spec.bits_per_sample as u16,
                cbSize: 0,
            };

            // Flags matching original: 0xE8 = DSBCAPS_LOCSOFTWARE(0x08) |
            // DSBCAPS_CTRLPAN(0x20) | DSBCAPS_CTRLFREQUENCY(0x40) | DSBCAPS_CTRLVOLUME(0x80)
            let desc = DSBUFFERDESC {
                dwSize: core::mem::size_of::<DSBUFFERDESC>() as u32,
                dwFlags: 0xE8,
                dwBufferBytes: buffer_bytes,
                dwReserved: 0,
                lpwfxFormat: &wfx as *const _ as *mut _,
                ..Default::default()
            };

            let ds = ds_from_field(core::ptr::addr_of!(self.direct_sound));
            let mut buf_opt: Option<IDirectSoundBuffer> = None;
            if ds.CreateSoundBuffer(&desc, &mut buf_opt, None).is_err() {
                self.close_file();
                return false;
            }
            let ds_buf = match buf_opt {
                Some(b) => b,
                None => {
                    self.close_file();
                    return false;
                }
            };

            // Store buffer as raw COM pointer (u32)
            self.ds_buffer = core::mem::transmute_copy::<IDirectSoundBuffer, u32>(&ds_buf);
            core::mem::forget(ds_buf); // Don't Release — we own the pointer now
            self.is_initialized = 1;
            self.flags |= streaming_flags::DS_BUFFER_ACTIVE;

            // Reset playback state
            self.write_cursor = 0;
            self.bytes_played = 0;
            self.file_bytes_read = 0;
            self.last_play_cursor = 0;
            self.bytes_played_snapshot = 0;
            self.timer_time_total = 0;
            self.avg_timer_time = 0;
            self.timer_call_count = 0;

            // Fill initial buffer and start playback
            self.fill_and_start()
        }
    }

    /// Fill the buffer with initial data and start playback + timer.
    /// Port of FUN_005751B0.
    unsafe fn fill_and_start(&mut self) -> bool {
        unsafe {
            if self.is_initialized == 0 || self.ds_buffer == 0 {
                return false;
            }

            let buf = ds_buf_from_field(core::ptr::addr_of!(self.ds_buffer));

            // Lock the entire buffer for initial fill
            let mut ptr1: *mut core::ffi::c_void = core::ptr::null_mut();
            let mut len1: u32 = 0;
            let mut ptr2: *mut core::ffi::c_void = core::ptr::null_mut();
            let mut len2: u32 = 0;

            if buf
                .Lock(
                    0,
                    self.ds_buffer_size,
                    &mut ptr1,
                    &mut len1,
                    Some(&mut ptr2),
                    Some(&mut len2),
                    0,
                )
                .is_err()
            {
                return false;
            }

            // Read from WAV file into buffer
            let bytes_read = self.read_file_data(core::slice::from_raw_parts_mut(
                ptr1 as *mut u8,
                len1 as usize,
            ));

            // Zero-fill remainder if we didn't fill the whole buffer
            if (bytes_read as u32) < len1 {
                core::ptr::write_bytes(
                    (ptr1 as *mut u8).add(bytes_read),
                    0,
                    (len1 as usize) - bytes_read,
                );
            }

            let _ = buf.Unlock(ptr1 as *const _, len1, Some(ptr2 as *const _), len2);

            self.write_cursor = bytes_read as u32;
            if self.write_cursor >= self.ds_buffer_size {
                self.write_cursor = 0;
            }

            // Start playback (looping the DS buffer — we manage the data ourselves)
            // DSBPLAY_LOOPING = 1
            let _ = buf.Play(0, 0, 1);

            // Set up the global pointer for the timer callback
            STREAMING_AUDIO_PTR.store(self as *mut Self as u32, Ordering::Release);

            // Start periodic timer using timeSetEvent
            let timer_id = time_set_event(
                self.timer_delay_ms,
                self.timer_resolution_ms,
                streaming_timer_callback,
                0,
                1, // TIME_PERIODIC
            );
            self.timer_id = timer_id;

            true
        }
    }

    /// Stop streaming playback and release resources. Port of FUN_00574CA0 + FUN_005752D0.
    pub unsafe fn stop(&mut self) {
        unsafe {
            // Kill timer first
            if self.timer_id != 0 {
                time_kill_event(self.timer_id);
                self.timer_id = 0;
            }

            // Clear the global pointer
            STREAMING_AUDIO_PTR.store(0, Ordering::Release);

            // Stop and release DS buffer
            if self.is_initialized != 0 {
                if self.ds_buffer != 0 {
                    let buf = ds_buf_from_field(core::ptr::addr_of!(self.ds_buffer));
                    let _ = buf.Stop();
                    // Release COM reference via transmute + drop
                    let com: IDirectSoundBuffer = core::mem::transmute_copy(&self.ds_buffer);
                    drop(com);
                    self.ds_buffer = 0;
                }
                self.is_initialized = 0;
            }

            self.flags &= !streaming_flags::DS_BUFFER_ACTIVE;
            self.close_file();

            // Reset state
            self.write_cursor = 0;
            self.bytes_played = 0;
            self.file_bytes_read = 0;
            self.last_play_cursor = 0;
            self.bytes_played_snapshot = 0;
        }
    }

    /// Close the WAV file handle.
    unsafe fn close_file(&mut self) {
        unsafe {
            if self.file_handle != 0 {
                let _ = Box::from_raw(self.file_handle as *mut std::io::BufReader<std::fs::File>);
                self.file_handle = 0;
            }
        }
    }

    /// Read PCM data from the WAV file. Reads until `buf` is full or EOF.
    /// Returns number of bytes actually read.
    unsafe fn read_file_data(&mut self, buf: &mut [u8]) -> usize {
        unsafe {
            if self.file_handle == 0 {
                return 0;
            }

            let file = &mut *(self.file_handle as *mut std::io::BufReader<std::fs::File>);
            let data_remaining = self.data_chunk.ck_size.saturating_sub(self.file_bytes_read);
            let to_read = buf.len().min(data_remaining as usize);

            if to_read == 0 {
                return 0;
            }

            // Loop to fill the entire requested amount (read() may return partial).
            let mut total = 0;
            while total < to_read {
                match file.read(&mut buf[total..to_read]) {
                    Ok(0) => break, // EOF
                    Ok(n) => total += n,
                    Err(_) => break,
                }
            }
            self.file_bytes_read += total as u32;
            total
        }
    }

    /// Seek back to the start of the PCM data (for looping).
    unsafe fn seek_to_data_start(&mut self) {
        unsafe {
            if self.file_handle == 0 {
                return;
            }
            let file = &mut *(self.file_handle as *mut std::io::BufReader<std::fs::File>);
            let _ = file.seek(SeekFrom::Start(self.data_chunk.data_offset as u64));
            self.file_bytes_read = 0;
        }
    }

    /// Timer callback handler — refills the DirectSound buffer.
    /// Port of FUN_005753A0 (the heart of streaming).
    unsafe fn timer_tick(&mut self) {
        unsafe {
            if self.is_initialized == 0 || self.ds_buffer == 0 {
                return;
            }

            let buf = ds_buf_from_field(core::ptr::addr_of!(self.ds_buffer));

            // Get current play position
            let mut play_cursor: u32 = 0;
            let mut _write_cursor: u32 = 0;
            if buf
                .GetCurrentPosition(Some(&mut play_cursor), Some(&mut _write_cursor))
                .is_err()
            {
                return;
            }

            // Calculate how many bytes have been played since last tick
            let played_delta = if play_cursor >= self.last_play_cursor {
                play_cursor - self.last_play_cursor
            } else {
                // Wraparound
                (self.ds_buffer_size - self.last_play_cursor) + play_cursor
            };
            self.last_play_cursor = play_cursor;
            self.bytes_played += played_delta;
            self.bytes_played_snapshot = self.bytes_played;
            self.timer_call_count += 1;

            // Check if we've reached end of stream (non-looping)
            let is_looping = self.flags & streaming_flags::LOOP != 0;
            if !is_looping
                && self.bytes_played >= self.data_chunk.ck_size.saturating_sub(self.fade_out_bytes)
            {
                self.flags |= streaming_flags::STOPPING;
            }

            if self.flags & streaming_flags::STOPPING != 0 {
                // Check if playback has finished
                if self.bytes_played >= self.data_chunk.ck_size {
                    // Signal completion via callback if set
                    if self.completion_callback != 0 {
                        // Original spawns a thread; we just call it directly since
                        // the callback is typically Music::advance_track
                        let cb: unsafe extern "system" fn(u32) -> u32 =
                            core::mem::transmute(self.completion_callback as usize);
                        cb(self.completion_callback_data);
                    }
                    return;
                }
            }

            // Calculate how much buffer space is available to fill
            let available = if play_cursor >= self.write_cursor {
                play_cursor - self.write_cursor
            } else {
                (self.ds_buffer_size - self.write_cursor) + play_cursor
            };

            if available == 0 {
                return;
            }

            // Lock the buffer region from write_cursor for `available` bytes
            let mut ptr1: *mut core::ffi::c_void = core::ptr::null_mut();
            let mut len1: u32 = 0;
            let mut ptr2: *mut core::ffi::c_void = core::ptr::null_mut();
            let mut len2: u32 = 0;

            if buf
                .Lock(
                    self.write_cursor,
                    available,
                    &mut ptr1,
                    &mut len1,
                    Some(&mut ptr2),
                    Some(&mut len2),
                    0,
                )
                .is_err()
            {
                return;
            }

            // Fill first region
            let mut total_written: u32 = 0;
            if len1 > 0 {
                let slice = core::slice::from_raw_parts_mut(ptr1 as *mut u8, len1 as usize);
                let read = self.read_file_data(slice);
                if (read as u32) < len1 {
                    if is_looping {
                        // Loop: seek back and read more
                        self.seek_to_data_start();
                        self.bytes_played = 0;
                        self.last_play_cursor = play_cursor;
                        let remaining = &mut slice[read..];
                        let extra = self.read_file_data(remaining);
                        total_written += read as u32 + extra as u32;
                        // Zero any remaining
                        if read + extra < len1 as usize {
                            core::ptr::write_bytes(
                                (ptr1 as *mut u8).add(read + extra),
                                0,
                                len1 as usize - read - extra,
                            );
                        }
                    } else {
                        // End of file: zero-fill remainder
                        core::ptr::write_bytes(
                            (ptr1 as *mut u8).add(read),
                            0,
                            len1 as usize - read,
                        );
                        total_written += len1;
                    }
                } else {
                    total_written += len1;
                }
            }

            // Fill second region (wraparound)
            if len2 > 0 {
                let slice = core::slice::from_raw_parts_mut(ptr2 as *mut u8, len2 as usize);
                let read = self.read_file_data(slice);
                if (read as u32) < len2 {
                    if is_looping {
                        self.seek_to_data_start();
                        self.bytes_played = 0;
                        self.last_play_cursor = play_cursor;
                        let remaining = &mut slice[read..];
                        let extra = self.read_file_data(remaining);
                        total_written += read as u32 + extra as u32;
                        if read + extra < len2 as usize {
                            core::ptr::write_bytes(
                                (ptr2 as *mut u8).add(read + extra),
                                0,
                                len2 as usize - read - extra,
                            );
                        }
                    } else {
                        core::ptr::write_bytes(
                            (ptr2 as *mut u8).add(read),
                            0,
                            len2 as usize - read,
                        );
                        total_written += len2;
                    }
                } else {
                    total_written += len2;
                }
            }

            let _ = buf.Unlock(ptr1 as *const _, len1, Some(ptr2 as *const _), len2);

            // Advance write cursor
            self.write_cursor = (self.write_cursor + total_written) % self.ds_buffer_size;
        }
    }

    /// Check if the DS buffer is currently playing.
    pub unsafe fn is_playing(&self) -> bool {
        unsafe {
            if self.is_initialized == 0 || self.ds_buffer == 0 {
                return false;
            }
            let buf = ds_buf_from_field(core::ptr::addr_of!(self.ds_buffer));
            match buf.GetStatus() {
                Ok(status) => status & 1 != 0, // DSBSTATUS_PLAYING
                Err(_) => false,
            }
        }
    }

    /// Set the playback volume. Volume is in centibels (0 = full, -10000 = silent).
    pub unsafe fn set_ds_volume(&self, volume_cb: i32) {
        unsafe {
            if self.is_initialized == 0 || self.ds_buffer == 0 {
                return;
            }
            let buf = ds_buf_from_field(core::ptr::addr_of!(self.ds_buffer));
            let _ = buf.SetVolume(volume_cb);
        }
    }
}

/// Timer callback invoked by timeSetEvent. Dispatches to the active StreamingAudio.
unsafe extern "system" fn streaming_timer_callback(
    _timer_id: u32,
    _msg: u32,
    _user: u32,
    _dw1: u32,
    _dw2: u32,
) {
    unsafe {
        let ptr = STREAMING_AUDIO_PTR.load(Ordering::Acquire);
        if ptr != 0 {
            let stream = &mut *(ptr as *mut StreamingAudio);
            stream.timer_tick();
        }
    }
}

// ============================================================
// Music implementation
// ============================================================

impl Music {
    /// Initialize a Music object. Port of constructor at 0x58BC10.
    ///
    /// `ds` is the IDirectSound instance (from DSSound).
    /// `base_path` is the music directory path (e.g. "DATA\\Music").
    pub unsafe fn init(this: *mut Music, ds: Ptr32, base_path: &str) {
        unsafe {
            // Zero the streaming sub-object
            (*this).streaming.init();

            // Set vtable — will be replaced by hook installation
            (*this).vtable = core::ptr::null();

            // Copy base path
            let bytes = base_path.as_bytes();
            let copy_len = bytes.len().min(255);
            core::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                (*this).base_path.as_mut_ptr() as *mut u8,
                copy_len,
            );
            (*this).base_path[copy_len] = 0;

            (*this).playlist_count = 0;
            (*this).current_track_index = 0;
            (*this).direct_sound = ds;
        }
    }

    /// Play a specific track by ID (1-13). Port of FUN_0058BD20.
    ///
    /// Maps the track ID to a WAV filename, builds the full path, and starts streaming.
    pub unsafe fn play_track(this: *mut Music, track_id: u32, do_loop: bool) {
        unsafe {
            if !(1..=13).contains(&track_id) {
                return;
            }

            let filename = MUSIC_TRACKS[(track_id - 1) as usize];

            // Build full path: base_path/filename
            let base = std::ffi::CStr::from_ptr((*this).base_path.as_ptr())
                .to_str()
                .unwrap_or("");
            let full_path = format!("{}/{}", base, filename);

            // Flags: 0x24 = AUTO_START | quality=2 (22kHz), plus LOOP if requested
            let mut flags: u32 = 0x24;
            if do_loop {
                flags |= streaming_flags::LOOP;
            }

            let _ = (*this)
                .streaming
                .open(&full_path, flags, (*this).direct_sound);
        }
    }

    /// Start a specific music track (looping). Port of vtable[1] at 0x58BE70.
    ///
    /// Clears the playlist (count=0) and plays the given track with loop=true.
    pub unsafe fn start_music(this: *mut Music, track_id: u32) {
        unsafe {
            (*this).playlist_count = 0;
            Music::play_track(this, track_id, true);
        }
    }

    /// Set a playlist and start the first track. Port of vtable[4] at 0x58BE90.
    ///
    /// Original passes loop=false (0) to PlayTrack — the track plays once,
    /// then advance_track is called via the completion callback.
    pub unsafe fn set_playlist(this: *mut Music, tracks: *const u32, count: u32) {
        unsafe {
            let count = count.min(32) as usize;
            core::ptr::copy_nonoverlapping(tracks, (*this).playlist.as_mut_ptr(), count);
            (*this).playlist_count = count as u32;
            (*this).current_track_index = 0;

            if count > 0 {
                let first_track = (*this).playlist[0];
                Music::play_track(this, first_track, false);
            }
        }
    }

    /// Advance to the next track in the playlist (wrapping). Port of vtable[1] at 0x58BEE0.
    ///
    /// The original checks if the DS buffer is still playing — if so, it returns
    /// without advancing. The game calls this speculatively (e.g., each turn),
    /// relying on the "still playing" guard to avoid premature track changes.
    pub unsafe fn advance_track(this: *mut Music) {
        unsafe {
            if (*this).playlist_count == 0 {
                return;
            }
            // Don't advance if the current track is still playing
            if (*this).streaming.is_playing() {
                return;
            }
            (*this).current_track_index =
                ((*this).current_track_index + 1) % (*this).playlist_count;
            let track_id = (*this).playlist[(*this).current_track_index as usize];
            Music::play_track(this, track_id, false);
        }
    }

    /// Set music volume. Port of vtable[2] at 0x58BF40.
    pub unsafe fn set_volume(this: *mut Music, volume: Fixed) {
        unsafe {
            let clamped = volume.clamp(Fixed::ZERO, Fixed::ONE);
            // Map to dB lookup table: index = volume * 64 / ONE
            let idx = ((clamped.0 as u64 * 64) / Fixed::ONE.0 as u64) as usize;
            let db = MUSIC_VOLUME_DB[idx.min(64)];
            (*this).streaming.set_ds_volume(db);
        }
    }

    /// Stop playback and clean up. Port of vtable[5] at 0x58BCE0.
    pub unsafe fn stop_and_cleanup(this: *mut Music) {
        unsafe {
            (*this).streaming.stop();
        }
    }

    /// Destructor. Port of 0x58BC80.
    pub unsafe fn destructor(this: *mut Music) {
        unsafe {
            // Original does Sleep(1) for thread safety — we rely on atomic ptr clearing in stop()
            (*this).streaming.stop();
        }
    }
}

// ============================================================
// Windows timer API bindings (timeSetEvent / timeKillEvent)
// ============================================================

#[link(name = "winmm")]
unsafe extern "system" {
    fn timeSetEvent(
        delay: u32,
        resolution: u32,
        callback: unsafe extern "system" fn(u32, u32, u32, u32, u32),
        user: u32,
        event_type: u32,
    ) -> u32;

    fn timeKillEvent(timer_id: u32) -> u32;
}

fn time_set_event(
    delay: u32,
    resolution: u32,
    callback: unsafe extern "system" fn(u32, u32, u32, u32, u32),
    user: u32,
    event_type: u32,
) -> u32 {
    unsafe { timeSetEvent(delay, resolution, callback, user, event_type) }
}

fn time_kill_event(timer_id: u32) -> u32 {
    unsafe { timeKillEvent(timer_id) }
}
