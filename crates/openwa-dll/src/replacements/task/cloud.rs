//! CTaskCloud vtable hooks and CreateWeatherFilter replacement.
//!
//! Thin hook shim — game logic lives in `openwa_core::task::cloud`.

use openwa_core::address::va;
use openwa_core::log::log_line;
use openwa_core::task::{cloud, team};

pub fn install() -> Result<(), String> {
    use openwa_core::vtable_replace;

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
