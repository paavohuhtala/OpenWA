//! CreateExplosion hook (0x548080).
//!
//! Thin hook shim — game logic lives in `openwa_game::game::create_explosion`.
//! This file contains the usercall trampoline and hook installation.

use openwa_core::fixed::Fixed;
use openwa_game::address::va;
use openwa_game::game::create_explosion as ce;
use openwa_game::task::BaseEntity;

use crate::hook;

// ── CreateExplosion (0x548080): usercall(EAX=pos_x, ECX=pos_y, ESI=sender)
//    + 4 stack params, RET 0x10 ──

#[unsafe(naked)]
unsafe extern "C" fn trampoline_create_explosion() {
    core::arch::naked_asm!(
        "push ebx",
        "push ebp",
        "push edi",
        // EAX=pos_x, ECX=pos_y, ESI=sender
        // Stack: 3 saves(12) + ret(4) = 16; original stack params at +16..+28
        "push [esp+28]",     // owner_id
        "push [esp+28]",     // zero
        "push [esp+28]",     // damage
        "push [esp+28]",     // explosion_id
        "push esi",          // sender (register param)
        "push ecx",          // pos_y (register param)
        "push eax",          // pos_x (register param)
        "call {impl_fn}",
        "add esp, 28",       // clean 7 cdecl args
        "pop edi",
        "pop ebp",
        "pop ebx",
        "ret 0x10",          // clean 4 original stack params
        impl_fn = sym create_explosion_cdecl,
    );
}

unsafe extern "cdecl" fn create_explosion_cdecl(
    pos_x: Fixed,
    pos_y: Fixed,
    sender: *mut BaseEntity,
    explosion_id: u32,
    damage: u32,
    caller_flag: u32,
    owner_id: u32,
) {
    unsafe {
        ce::create_explosion(
            pos_x,
            pos_y,
            sender,
            explosion_id,
            damage,
            caller_flag,
            owner_id,
        );
    }
}

pub fn install() -> Result<(), String> {
    unsafe {
        hook::install(
            "CreateExplosion",
            va::CREATE_EXPLOSION,
            trampoline_create_explosion as *const (),
        )?;
    }
    Ok(())
}
