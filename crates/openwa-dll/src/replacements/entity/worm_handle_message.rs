//! `WormEntity::HandleMessage` (vtable slot 2 of `WORM_ENTITY_VTABLE`).
//!
//! Thin shim — game logic lives in `openwa_game::entity::worm_handle_message`.
//! Replaces vtable slot 2 with the Rust dispatcher, saving WA's original
//! function pointer so unported message branches fall through to it.
//!
//! Also installs the `WormEntity::CanIdleSound_Maybe` (0x0050E5E0)
//! replacement since both Rust callers (the case 0x5 dispatch) and the
//! WA-side caller in `WormEntity::BehaviorTick` need to land on the
//! ported impl.

use crate::hook;
use openwa_game::address::va;
use openwa_game::entity::{worm::WormEntityVtable, worm_handle_message};

// usercall(EAX=this) trampoline for `WormEntity::CanIdleSound_Maybe`.
// Captures `this` from EAX and forwards it to the cdecl impl in
// `openwa-game`. The impl returns `i32` in EAX, which the trampoline
// preserves through the cdecl call.
hook::usercall_trampoline!(
    fn worm_can_idle_sound_trampoline;
    impl_fn = openwa_game::entity::worm::worm_can_idle_sound_impl;
    reg = eax
);

pub fn install() -> Result<(), String> {
    use openwa_game::vtable_replace;

    unsafe { worm_handle_message::init_addrs() };

    vtable_replace!(WormEntityVtable, va::WORM_ENTITY_VTABLE, {
        handle_message [worm_handle_message::ORIGINAL_HANDLE_MESSAGE]
            => worm_handle_message::handle_message,
    })?;

    unsafe {
        hook::install(
            "WormEntity__CanIdleSound",
            va::WORM_ENTITY_CAN_IDLE_SOUND,
            worm_can_idle_sound_trampoline as *const (),
        )?;
    }

    Ok(())
}
