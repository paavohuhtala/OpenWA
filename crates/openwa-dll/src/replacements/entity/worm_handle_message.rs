//! `WormEntity::HandleMessage` (vtable slot 2 of `WORM_ENTITY_VTABLE`).
//!
//! Thin shim — game logic lives in `openwa_game::entity::worm_handle_message`.
//! Replaces vtable slot 2 with the Rust dispatcher, saving WA's original
//! function pointer so unported message branches fall through to it.
//!
//! Also installs the `WormEntity__CanIdleSound` (0x0050E5E0)
//! replacement since both Rust callers (the case 0x5 dispatch) and the
//! WA-side caller in `WormEntity::BehaviorTick` need to land on the
//! ported impl.

use openwa_game::address::va;
use openwa_game::entity::{worm::WormEntityVtable, worm_handle_message};

pub fn install() -> Result<(), String> {
    use openwa_game::vtable_replace;

    unsafe { worm_handle_message::init_addrs() };

    vtable_replace!(WormEntityVtable, va::WORM_ENTITY_VTABLE, {
        handle_message [worm_handle_message::ORIGINAL_HANDLE_MESSAGE]
            => worm_handle_message::handle_message,
    })?;

    unsafe {
        crate::generated::hooks::install_WormEntity__CanIdleSound()?;
    }

    Ok(())
}
