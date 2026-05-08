//! Mine entity hook installation.
//!
//! Thin shim — game logic lives in `openwa_game::entity::mine`.
//! Initializes the bridge addresses each submodule needs, then replaces
//! `MineEntity` vtable slots that have a Rust port (slot 1 `free`,
//! slot 2 `handle_message`).

use openwa_game::address::va;
use openwa_game::entity::mine::{self, MineEntityVtable};

pub fn install() -> Result<(), String> {
    use openwa_game::vtable_replace;

    unsafe { mine::handle_message::init_addrs() };
    unsafe { mine::render::init_addrs() };
    unsafe { mine::constructor::init_addrs() };

    vtable_replace!(MineEntityVtable, va::MINE_ENTITY_VTABLE, {
        handle_message [mine::handle_message::ORIGINAL_HANDLE_MESSAGE]
            => mine::handle_message::handle_message,
        free => mine::handle_message::free,
    })?;

    Ok(())
}
