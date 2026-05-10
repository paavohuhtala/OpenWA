//! Oil drum entity hook installation.
//!
//! Thin shim — game logic lives in `openwa_game::entity::oil_drum`.
//! Initialises the bridge addresses each submodule needs, then replaces
//! `OilDrumEntity` vtable slots that have a Rust port (slot 1 `free`,
//! slot 2 `handle_message`).

use openwa_game::address::va;
use openwa_game::entity::oil_drum::{self, OilDrumEntityVtable};

pub fn install() -> Result<(), String> {
    use openwa_game::vtable_replace;

    unsafe { oil_drum::handle_message::init_addrs() };
    unsafe { oil_drum::render::init_addrs() };

    vtable_replace!(OilDrumEntityVtable, va::OILDRUM_ENTITY_VTABLE, {
        handle_message [oil_drum::handle_message::ORIGINAL_HANDLE_MESSAGE]
            => oil_drum::handle_message::handle_message,
        free => oil_drum::handle_message::free,
    })?;

    Ok(())
}
