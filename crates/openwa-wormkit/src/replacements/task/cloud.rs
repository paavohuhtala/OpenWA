//! CTaskCloud vtable hooks.
//!
//! Thin wormkit shim — game logic lives in `openwa_core::task::cloud::cloud_handle_message`.

use openwa_core::address::va;
use openwa_core::log::log_line;
use openwa_core::task::cloud;

pub fn install() -> Result<(), String> {
    use openwa_core::vtable_replace;

    vtable_replace!(cloud::CTaskCloudVTable, va::CTASK_CLOUD_VTABLE, {
        handle_message => cloud::cloud_handle_message,
    })?;

    let _ = log_line("[Cloud] HandleMessage hooked via vtable_replace");
    Ok(())
}
