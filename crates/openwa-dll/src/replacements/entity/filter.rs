//! FilterEntity vtable hooks.
//!
//! Thin hook shim — game logic lives in `openwa_game::entity::filter::filter_handle_message`.

use openwa_game::address::va;
use openwa_game::entity::filter;

pub fn install() -> Result<(), String> {
    use openwa_game::vtable_replace;

    vtable_replace!(filter::FilterEntityVtable, va::FILTER_ENTITY_VTABLE, {
        handle_message => filter::filter_handle_message,
    })?;

    Ok(())
}
