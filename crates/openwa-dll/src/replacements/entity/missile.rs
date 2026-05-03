//! MissileEntity vtable hooks.
//!
//! Thin hook shim — game logic lives in `openwa_game::game::missile_contact`.

use openwa_core::log::log_line;
use openwa_game::address::va;
use openwa_game::entity::missile;
use openwa_game::game::missile_contact;

pub fn install() -> Result<(), String> {
    use openwa_game::vtable_replace;

    vtable_replace!(missile::MissileEntityVtable, va::MISSILE_ENTITY_VTABLE, {
        on_contact => missile_contact::missile_on_contact,
    })?;

    let _ = log_line("[Missile] OnContact hooked");
    Ok(())
}
