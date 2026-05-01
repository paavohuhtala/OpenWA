//! `WormEntity::HandleMessage` (vtable slot 2 of `WORM_ENTITY_VTABLE`).
//!
//! Thin shim — game logic lives in `openwa_game::task::worm_handle_message`.
//! Replaces vtable slot 2 with the Rust dispatcher, saving WA's original
//! function pointer so unported message branches fall through to it.

use openwa_core::log::log_line;
use openwa_game::address::va;
use openwa_game::task::{worm::WormEntityVtable, worm_handle_message};

pub fn install() -> Result<(), String> {
    use openwa_game::vtable_replace;

    vtable_replace!(WormEntityVtable, va::WORM_ENTITY_VTABLE, {
        handle_message [worm_handle_message::ORIGINAL_HANDLE_MESSAGE]
            => worm_handle_message::handle_message,
    })?;

    let _ = log_line(
        "[Worm] HandleMessage vtable hooked (Rust dispatcher with WA fall-through for unported branches)",
    );
    Ok(())
}
