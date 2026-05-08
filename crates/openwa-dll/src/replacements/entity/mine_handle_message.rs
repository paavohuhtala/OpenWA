//! `MineEntity::HandleMessage` (vtable slot 2 of `MINE_ENTITY_VTABLE`).
//!
//! Thin shim — game logic lives in `openwa_game::entity::mine_handle_message`.
//! Replaces vtable slot 2 with the Rust dispatcher, saving WA's original
//! function pointer so unported message branches fall through to it.

use openwa_game::address::va;
use openwa_game::entity::{
    mine::MineEntityVtable, mine_constructor, mine_handle_message, mine_render,
};

pub fn install() -> Result<(), String> {
    use openwa_game::vtable_replace;

    unsafe { mine_handle_message::init_addrs() };
    unsafe { mine_render::init_addrs() };
    unsafe { mine_constructor::init_addrs() };

    vtable_replace!(MineEntityVtable, va::MINE_ENTITY_VTABLE, {
        handle_message [mine_handle_message::ORIGINAL_HANDLE_MESSAGE]
            => mine_handle_message::handle_message,
        free => mine_handle_message::free,
    })?;

    Ok(())
}
