//! CTaskCloud vtable hooks and CreateWeatherFilter replacement.
//!
//! Thin hook shim — game logic lives in `openwa_game::task::cloud`.

use openwa_game::address::va;
use openwa_game::log::log_line;
use openwa_game::task::{cloud, team};

pub fn install() -> Result<(), String> {
    use openwa_game::vtable_replace;

    vtable_replace!(cloud::CTaskCloudVTable, va::CTASK_CLOUD_VTABLE, {
        handle_message => cloud::cloud_handle_message,
    })?;

    unsafe {
        crate::hook::install(
            "CTaskTeam__CreateWeatherFilter",
            va::CTASK_TEAM_CREATE_WEATHER_FILTER,
            team::create_weather_filter as *const (),
        )?;
    }

    let _ = log_line("[Cloud] HandleMessage + CreateWeatherFilter hooked");
    Ok(())
}
