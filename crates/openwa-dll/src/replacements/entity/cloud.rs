//! CloudEntity vtable hooks and CreateWeatherFilter replacement.
//!
//! Thin hook shim — game logic lives in `openwa_game::entity::cloud`.

use openwa_core::log::log_line;
use openwa_game::address::va;
use openwa_game::entity::{cloud, team};

pub fn install() -> Result<(), String> {
    use openwa_game::vtable_replace;

    vtable_replace!(cloud::CloudEntityVtable, va::CLOUD_ENTITY_VTABLE, {
        handle_message => cloud::cloud_handle_message,
    })?;

    unsafe {
        crate::hook::install(
            "TeamEntity__CreateWeatherFilter",
            va::TEAM_ENTITY_CREATE_WEATHER_FILTER,
            team::create_weather_filter as *const (),
        )?;
    }

    let _ = log_line("[Cloud] HandleMessage + CreateWeatherFilter hooked");
    Ok(())
}
