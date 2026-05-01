//! Hooks for `WorldEntity::HandleMessage` (0x004FF280) and its three
//! formerly-bridged helpers. Logic lives in
//! `openwa_game::game::game_task_message`.

use openwa_core::fixed::Fixed;
use openwa_game::address::va;
use openwa_game::game::{EntityMessage, game_task_message as gtm};
use openwa_game::task::{BaseEntity, WorldEntity};

use crate::hook::{self, usercall_trampoline};

usercall_trampoline!(fn trampoline_cgametask_handle_message;
    impl_fn = cgametask_handle_message_impl;
    reg = ecx; stack_params = 4; ret_bytes = "0x10");

unsafe extern "cdecl" fn cgametask_handle_message_impl(
    this: *mut WorldEntity,
    sender: *mut BaseEntity,
    msg_type: EntityMessage,
    size: u32,
    data: *const u8,
) {
    unsafe {
        gtm::cgametask_handle_message(this, sender, msg_type, size, data);
    }
}

usercall_trampoline!(fn trampoline_is_sound_handle_expired;
    impl_fn = is_sound_handle_expired_impl;
    regs = [ecx, eax]);

unsafe extern "cdecl" fn is_sound_handle_expired_impl(
    this: *const WorldEntity,
    handle: u32,
) -> u32 {
    unsafe { gtm::sound_handle_expired(this, handle) }
}

// EDI is LLVM-reserved on x86, so the macro can't capture it — write the
// trampoline by hand. The cdecl impl preserves EDI per ABI, so the WA
// caller's `this` register survives the call without an explicit save.
#[unsafe(naked)]
unsafe extern "C" fn trampoline_compute_explosion_damage() {
    core::arch::naked_asm!(
        "push [esp+16]",
        "push [esp+16]",
        "push [esp+16]",
        "push [esp+16]",
        "push edi",
        "call {impl_fn}",
        "add esp, 20",
        "ret 0x10",
        impl_fn = sym compute_explosion_damage_impl,
    );
}

unsafe extern "cdecl" fn compute_explosion_damage_impl(
    this: *mut WorldEntity,
    strength: u32,
    damage: u32,
    pos_x: Fixed,
    pos_y: Fixed,
) -> i32 {
    unsafe { gtm::compute_explosion_damage(this, strength, damage, pos_x, pos_y) }
}

pub fn install() -> Result<(), String> {
    unsafe {
        hook::install(
            "WorldEntity::HandleMessage",
            va::CGAMETASK_VT2_HANDLE_MESSAGE,
            trampoline_cgametask_handle_message as *const (),
        )?;
        hook::install(
            "WorldEntity::IsSoundHandleExpired",
            va::WORLD_ENTITY_IS_SOUND_HANDLE_EXPIRED,
            trampoline_is_sound_handle_expired as *const (),
        )?;
        hook::install(
            "WorldEntity::ReleaseSoundHandle",
            va::WORLD_ENTITY_RELEASE_SOUND_HANDLE,
            gtm::release_sound_handle as *const (),
        )?;
        hook::install(
            "WorldEntity::ComputeExplosionDamage",
            va::WORLD_ENTITY_COMPUTE_EXPLOSION_DAMAGE,
            trampoline_compute_explosion_damage as *const (),
        )?;
    }
    Ok(())
}
