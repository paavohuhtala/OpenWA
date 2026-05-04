//! Landscape vtable hooks.
//!
//! Thin shim — game logic lives in
//! [`openwa_game::render::landscape::landscape_init_borders_impl`].

use openwa_game::address::va;
use openwa_game::render::landscape;

pub fn install() -> Result<(), String> {
    use openwa_game::vtable_replace;

    vtable_replace!(landscape::LandscapeVtable, va::LANDSCAPE_VTABLE, {
        init_borders => landscape::landscape_init_borders_impl,
    })?;

    Ok(())
}
