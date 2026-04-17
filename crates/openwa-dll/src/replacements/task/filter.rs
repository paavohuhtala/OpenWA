//! CTaskFilter vtable hooks.
//!
//! Thin hook shim — game logic lives in `openwa_game::task::filter::filter_handle_message`.

use openwa_core::log::log_line;
use openwa_game::address::va;
use openwa_game::task::filter;

pub fn install() -> Result<(), String> {
    use openwa_game::vtable_replace;

    vtable_replace!(filter::CTaskFilterVTable, va::CTASK_FILTER_VTABLE, {
        handle_message => filter::filter_handle_message,
    })?;

    let _ = log_line("[Filter] HandleMessage hooked via vtable_replace");
    Ok(())
}
