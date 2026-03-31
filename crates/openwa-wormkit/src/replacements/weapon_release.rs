//! WeaponRelease hook (0x51C3D0) and SpawnEffect (0x547C30).
//!
//! Thin wormkit shim — game logic lives in `openwa_core::game::weapon_release`.
//! This file contains usercall trampolines and hook installation.

use openwa_core::address::va;
use openwa_core::fixed::Fixed;
use openwa_core::game::weapon_release as wr;
use openwa_core::task::worm::CTaskWorm;

use crate::hook::{self, usercall_trampoline};

// ── WeaponRelease (0x51C3D0): usercall(EAX=worm) + 4 stack, RET 0x10 ──

usercall_trampoline!(fn trampoline_weapon_release; impl_fn = weapon_release_impl;
    reg = eax; stack_params = 4; ret_bytes = "0x10");

unsafe extern "cdecl" fn weapon_release_impl(
    worm: *mut CTaskWorm,
    spawn_x: u32,
    spawn_y: u32,
    aim_dir_x: Fixed,
    aim_dir_y: Fixed,
) {
    wr::weapon_release(worm, spawn_x, spawn_y, aim_dir_x, aim_dir_y);
}

// ── SpawnEffect (0x547C30): usercall(EAX=const, ECX=speed_x, ESI=worm) + 7 stack ──

#[unsafe(naked)]
unsafe extern "C" fn trampoline_spawn_effect() {
    core::arch::naked_asm!(
        "push ebx",
        "push ebp",
        "push edi",
        // ESI=worm, EAX=constant, ECX=speed_x
        // Stack: 3 saves(12) + ret(4) = 16; original stack params at +16
        "push [esp+40]",      // scale
        "push [esp+40]",      // size
        "push [esp+40]",      // state_flag
        "push [esp+40]",      // palette
        "push [esp+40]",      // rng_offset
        "push [esp+40]",      // rng_scaled
        "push [esp+40]",      // speed_y
        "push ecx",           // speed_x (register param)
        "push eax",           // constant (register param)
        "push esi",           // worm (register param)
        "call {impl_fn}",
        "add esp, 40",        // clean 10 cdecl args
        "pop edi",
        "pop ebp",
        "pop ebx",
        "ret 0x1C",           // clean 7 original stack params
        impl_fn = sym spawn_effect_cdecl,
    );
}

unsafe extern "cdecl" fn spawn_effect_cdecl(
    worm: *mut CTaskWorm,
    constant: u32,
    speed_x: Fixed,
    speed_y: Fixed,
    rng_scaled: i32,
    rng_offset: i32,
    palette: u32,
    state_flag: u32,
    size: Fixed,
    scale: Fixed,
) {
    wr::spawn_effect(
        worm, constant, speed_x, speed_y, rng_scaled, rng_offset, palette, state_flag, size, scale,
    );
}

// ── Hook installation ──

pub fn install() -> Result<(), String> {
    unsafe {
        hook::install(
            "SpawnEffect",
            va::SPAWN_EFFECT,
            trampoline_spawn_effect as *const (),
        )?;
        hook::install(
            "WeaponRelease",
            va::WEAPON_RELEASE,
            trampoline_weapon_release as *const (),
        )?;
    }
    Ok(())
}
