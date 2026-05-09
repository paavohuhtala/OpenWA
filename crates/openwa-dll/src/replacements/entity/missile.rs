//! MissileEntity vtable hooks.
//!
//! Thin hook shim — game logic lives in `openwa_game::entity::missile` and
//! `openwa_game::game::missile_contact`.

use openwa_game::address::va;
use openwa_game::entity::missile;
use openwa_game::game::missile_contact;

pub fn install() -> Result<(), String> {
    use openwa_game::vtable_replace;

    unsafe { missile::handle_message::init_addrs() };

    vtable_replace!(missile::MissileEntityVtable, va::MISSILE_ENTITY_VTABLE, {
        handle_message [missile::handle_message::ORIGINAL_HANDLE_MESSAGE]
            => missile::handle_message::handle_message,
        on_contact => missile_contact::missile_on_contact,
    })?;

    Ok(())
}
