//! Music playback hooks.
//!
//! Replaces WA's Music vtable (0x66B3E0, 6 slots) with Rust implementations
//! from `openwa_game::audio::music`.
//!
//! The Music system handles background music during gameplay:
//! - Track selection from playlist (track IDs 1-13 → WAV filenames)
//! - Streaming WAV playback via DirectSound double-buffering
//! - Volume control with dB lookup table
//! - Playlist cycling (advance to next track when current finishes)

use openwa_game::address::va;
use openwa_game::audio::music::{Music, MusicVtable};
use openwa_game::fixed::Fixed;
use openwa_game::rebase::rb;
use openwa_game::wa_alloc::wa_free;

use crate::hook;
use crate::log_line;

// ── Music vtable replacement methods ──

/// Slot 0: scalar_deleting_dtor — calls destructor, optionally frees.
unsafe extern "thiscall" fn hook_scalar_deleting_dtor(this: *mut Music, flags: u32) {
    unsafe {
        let _ = log_line(&format!(
            "[Music] scalar_deleting_dtor: this=0x{:08X} flags={}",
            this as u32, flags
        ));
        Music::destructor(this);
        if flags & 1 != 0 {
            // Free via WA's operator delete (0x5C0AE8) — matches WABox allocation.
            wa_free(this as *mut u8);
        }
    }
}

/// Slot 1: start_music — start a specific track (looping).
unsafe extern "thiscall" fn hook_start_music(this: *mut Music, track_id: u32) {
    unsafe {
        let _ = log_line(&format!("[Music] start_music: track_id={}", track_id));
        Music::start_music(this, track_id);
    }
}

/// Slot 2: set_playlist — copy track IDs and start first track.
unsafe extern "thiscall" fn hook_set_playlist(this: *mut Music, tracks: *const u32, count: u32) {
    unsafe {
        let _ = log_line(&format!("[Music] set_playlist: count={}", count));
        Music::set_playlist(this, tracks, count);
    }
}

/// Slot 3: advance_track — next track in playlist (wrapping).
unsafe extern "thiscall" fn hook_advance_track(this: *mut Music) {
    unsafe {
        let _ = log_line("[Music] advance_track");
        Music::advance_track(this);
    }
}

/// Slot 4: set_volume — set music volume (Fixed 16.16).
unsafe extern "thiscall" fn hook_set_volume(this: *mut Music, volume: Fixed) {
    unsafe {
        Music::set_volume(this, volume);
    }
}

/// Slot 5: stop_and_cleanup — stop playback.
unsafe extern "thiscall" fn hook_stop_and_cleanup(this: *mut Music) {
    unsafe {
        let _ = log_line("[Music] stop_and_cleanup");
        Music::stop_and_cleanup(this);
    }
}

// ── Music constructor hook ──

// Hook for Music::Constructor (0x58BC10).
// usercall(ESI=this) + stack(IDirectSound*, path_ptr), RET 0x8.
hook::usercall_trampoline!(
    fn trampoline_music_constructor;
    impl_fn = music_constructor_cdecl;
    reg = esi;
    stack_params = 2; ret_bytes = "0x8"
);

unsafe extern "cdecl" fn music_constructor_cdecl(this: *mut Music, ids: u32, path_ptr: *const u8) {
    unsafe {
        let path = std::ffi::CStr::from_ptr(path_ptr as *const i8)
            .to_str()
            .unwrap_or("DATA\\Music");
        let _ = log_line(&format!(
            "[Music] Constructor: this=0x{:08X} ids=0x{:08X} path=\"{}\"",
            this as u32, ids, path
        ));
        Music::init(this, ids, path);
        // Set vtable to the patched version (points to our hooked functions)
        (*this).vtable = rb(va::MUSIC_VTABLE) as *const MusicVtable;
    }
}

// ── Hook installation ──

pub fn install() -> Result<(), String> {
    unsafe {
        // Patch the Music vtable using vtable_replace!
        use openwa_game::audio::music::MusicVtable;
        use openwa_game::vtable_replace;

        vtable_replace!(MusicVtable, va::MUSIC_VTABLE, {
            scalar_deleting_dtor => hook_scalar_deleting_dtor,
            advance_track        => hook_advance_track,
            set_volume           => hook_set_volume,
            start_music          => hook_start_music,
            set_playlist         => hook_set_playlist,
            stop_and_cleanup     => hook_stop_and_cleanup,
        })?;

        let _ = log_line("[Music] vtable: patched 6/6 slots with Rust");

        // Hook the Music constructor
        let _ = hook::install(
            "Music_Constructor",
            va::MUSIC_CONSTRUCTOR,
            trampoline_music_constructor as *const (),
        )?;

        // Trap the internal destructor (only called from our scalar_deleting_dtor)
        hook::install_trap!("Music_Destructor", va::MUSIC_DESTRUCTOR);
        // Trap PlayTrack (only called from our vtable methods)
        hook::install_trap!("Music_PlayTrack", va::MUSIC_PLAY_TRACK);

        // NOTE: StreamingAudio functions have WA callers outside the Music system
        // (GameEngine::Shutdown calls Stop, Init is used generically).
        // These can only be trapped once the shutdown path is also ported.
    }

    Ok(())
}
