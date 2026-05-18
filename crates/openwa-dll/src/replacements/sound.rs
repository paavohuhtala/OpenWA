//! Sound playback hooks. Thin shim — game logic lives in
//! `openwa_game::audio::sound_ops`. Per-hook calling conventions live in
//! `re/**/*.toml`; generated install helpers in `crate::generated::hooks`.

use std::sync::atomic::Ordering;

use openwa_core::fixed::Fixed;
use openwa_game::address::va;
use openwa_game::audio::sound_ops;
use openwa_game::audio::{KnownSoundId, SoundId};
use openwa_game::engine::{GameRuntime, GameWorld};
use openwa_game::entity::worm::WormEntity;
use openwa_game::entity::{BaseEntity, WorldEntity};

use crate::hook;
use crate::log_line;

/// Whether sound logging is enabled (checked once at init).
static SOUND_LOG_ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// ── PlaySoundGlobal (0x546E20): thiscall(ECX=BaseEntity*, 4 stack, RET 0x10) ──

unsafe extern "thiscall" fn hook_play_sound_global(
    this: *const WorldEntity,
    sound_id: SoundId,
    flags: u32,
    volume: Fixed,
    pitch: Fixed,
) -> u32 {
    unsafe {
        if SOUND_LOG_ENABLED.load(Ordering::Relaxed) {
            let sound_name = KnownSoundId::try_from(sound_id.0)
                .map(|s| format!("{s:?}"))
                .unwrap_or_else(|v| format!("#{v}"));
            let _ = log_line(&format!(
                "[Sound] Global: entity=0x{this:08X?} id={sound_id:?}({sound_name}) \
             p3={flags} p4={volume} p5={pitch}"
            ));
        }

        sound_ops::queue_sound((*this).base.world, sound_id, flags, volume, pitch).is_some() as u32
    }
}

// ── PlaySoundLocal (0x4FDFE0): usercall(EAX=pitch, ECX=volume, EDI=entity, stack) ──

pub(crate) unsafe extern "cdecl" fn play_sound_local_impl(
    pitch: Fixed,
    volume: Fixed,
    entity: *mut WorldEntity,
    sound_id: SoundId,
    flags: u32,
) -> u32 {
    unsafe {
        if SOUND_LOG_ENABLED.load(Ordering::Relaxed) {
            let sound_name = KnownSoundId::try_from(sound_id.0)
                .map(|s| format!("{s:?}"))
                .unwrap_or_else(|v| format!("#{v}"));
            let _ = log_line(&format!(
                "[Sound] Local: pitch={pitch} volume={volume} entity=0x{entity:08X?} \
             id={sound_id:?}({sound_name}) flags={flags}"
            ));
        }

        sound_ops::play_sound_local(entity, sound_id, flags, volume, pitch) as u32
    }
}

// ── WormEntity__PlaySound (0x515020): usercall(EDI=worm) + stdcall(sound_id, volume, flags) ──

pub(crate) unsafe extern "cdecl" fn play_worm_sound_2_cdecl(
    worm: *mut WormEntity,
    sound_id: SoundId,
    volume: Fixed,
    flags: u32,
) {
    unsafe {
        sound_ops::play_worm_sound_2(worm, sound_id, volume, flags);
    }
}

// ── IsSoundSuppressed (0x5261E0): thiscall(ECX=GameWorld*) ──

pub(crate) unsafe extern "cdecl" fn hook_is_sound_suppressed(world: *mut GameWorld) -> u32 {
    unsafe { sound_ops::is_sound_suppressed(world) as u32 }
}

// ── DispatchGlobalSound (0x526270): fastcall(ECX=unused, EDX=wrapper) + 4 stack ──

unsafe extern "fastcall" fn hook_dispatch_global_sound(
    _ecx: u32,
    runtime: *const GameRuntime,
    slot: SoundId,
    priority: i32,
    frequency: Fixed,
    volume: Fixed,
) -> u32 {
    unsafe { sound_ops::dispatch_global_sound(runtime, slot, priority, frequency, volume) }
}

// ── PlaySoundPooled_Direct (0x546B50): fastcall(ECX=unused, EDX=entity) + 3 stack ──

unsafe extern "fastcall" fn hook_play_sound_pooled_direct(
    _ecx: u32,
    entity: *const BaseEntity,
    slot: SoundId,
    priority: i32,
    volume: Fixed,
) -> i32 {
    unsafe { sound_ops::play_sound_pooled_direct(entity, slot, priority, volume) }
}

// ── WormEntity__fire_sound (0x5150D0): usercall(EDI=worm) + stack(sound_id, volume), RET 0x8 ──

pub(crate) unsafe extern "cdecl" fn play_worm_sound_cdecl(
    worm: *mut WormEntity,
    sound_id: SoundId,
    volume: Fixed,
) {
    unsafe {
        sound_ops::play_worm_sound(worm, sound_id, volume);
    }
}

// ── StopWormSound (0x515180): usercall(ESI=worm), plain RET ──

pub(crate) unsafe extern "cdecl" fn stop_worm_sound_cdecl(worm: *mut WormEntity) {
    unsafe {
        sound_ops::stop_worm_sound(worm);
    }
}

// ── LoadAndPlayStreaming (0x546C20): usercall(EAX=entity) + stack(sound_id, flags, volume), RET 0xC ──

pub(crate) unsafe extern "cdecl" fn load_and_play_streaming_cdecl(
    entity: *mut WorldEntity,
    sound_id: SoundId,
    flags: u32,
    volume: Fixed,
) -> i32 {
    unsafe { sound_ops::load_and_play_streaming(entity, sound_id, flags, volume) }
}

// ── Hook installation ──

pub fn install() -> Result<(), String> {
    if std::env::var("OPENWA_SOUND_LOG").is_ok() {
        SOUND_LOG_ENABLED.store(true, Ordering::Relaxed);
        let _ = log_line("[Sound] Logging enabled (OPENWA_SOUND_LOG=1)");
    }

    unsafe {
        let _ = hook::install(
            "PlaySoundGlobal",
            va::PLAY_SOUND_GLOBAL,
            hook_play_sound_global as *const (),
        )?;
        crate::generated::hooks::install_PlaySoundLocal()?;
        crate::generated::hooks::install_GameWorld__IsSoundSuppressed()?;
        let _ = hook::install(
            "DispatchGlobalSound",
            va::DISPATCH_GLOBAL_SOUND,
            hook_dispatch_global_sound as *const (),
        )?;
        let _ = hook::install(
            "PlaySoundPooled_Direct",
            va::PLAY_SOUND_POOLED_DIRECT,
            hook_play_sound_pooled_direct as *const (),
        )?;

        // Patch DSSound vtable: replace all 24 slots with Rust implementations.
        patch_dssound_vtable()?;

        // Hook WormEntity::PlaySound2 (WormEntity__PlaySound) — 23 callers in WA
        crate::generated::hooks::install_WormEntity__PlaySound()?;

        // Trap: only caller (PlayWormSound2) is fully ported Rust
        hook::install_trap!(
            "LoadAndPlayStreamingPositional",
            va::LOAD_AND_PLAY_STREAMING_POSITIONAL
        );

        // Hook PlayWormSound + StopWormSound — these have WA callers beyond weapon_release.rs
        crate::generated::hooks::install_WormEntity__fire_sound()?;
        crate::generated::hooks::install_WormEntity__stop_fire_sound()?;

        // NOTE: Sound sub-functions (Distance3D_Attenuation, ActiveSoundTable::stop_sound,
        // RecordActiveSound, DispatchLocalSound, ComputeDistanceParams) all have WA callers
        // through unported paths (PlayLocalNoEmitter, PlayLocalWithEmitter) that are
        // exercised in headful mode. Cannot trap until those entry points are also hooked.

        // Hook LoadAndPlayStreaming — has many WA callers (MissileEntity, etc.)
        crate::generated::hooks::install_LoadAndPlayStreaming()?;
    }

    Ok(())
}

/// Patch DSSound vtable (0x66AF20) to replace trivial methods with Rust.
unsafe fn patch_dssound_vtable() -> Result<(), &'static str> {
    use openwa_game::audio::{
        DSSoundVtable, dssound_destructor, dssound_noop, dssound_returns_0, dssound_returns_1,
        dssound_sub_destructor, is_channel_finished, is_slot_loaded, load_wav, play_sound,
        play_sound_pooled, release_finished, set_channel_volume, set_frequency_scale,
        set_master_volume, set_pan, stop_channel, update_channels,
    };
    use openwa_game::vtable_replace;

    vtable_replace!(DSSoundVtable, va::DS_SOUND_VTABLE, {
        destructor          => dssound_destructor,
        update_channels     => update_channels,
        set_frequency_scale => set_frequency_scale,
        play_sound          => play_sound,
        play_sound_pooled   => play_sound_pooled,
        set_pan             => set_pan,
        stub_6              => dssound_returns_0,
        set_master_volume   => set_master_volume,
        set_channel_volume  => set_channel_volume,
        is_channel_finished => is_channel_finished,
        stop_channel        => stop_channel,
        release_finished    => release_finished,
        load_wav            => load_wav,
        is_slot_loaded      => is_slot_loaded,
        sub_destructor      => dssound_sub_destructor,
        noop_15             => dssound_noop,
        noop_16             => dssound_noop,
        returns_0_17        => dssound_returns_0,
        returns_0_18        => dssound_returns_0,
        stub_19             => dssound_returns_0,
        stub_20             => dssound_returns_0,
        returns_0_21        => dssound_returns_0,
        stub_22             => dssound_returns_0,
        returns_1_23        => dssound_returns_1,
    })?;

    Ok(())
}
