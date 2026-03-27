mod config;
mod ddgame_init;
mod display;
mod file_isolation;
mod frontend;
mod game_session;
mod game_state_hooks;
mod hardware_init;
mod headless;
pub(crate) mod input;
mod render;
mod replay;
mod scheme;
mod sound;
mod speech;
mod task;
mod team;
mod weapon;
mod weapon_release;

/// Write gameplay milestone report. Called from DLL_PROCESS_DETACH.
pub fn write_gameplay_report() {
    input::write_gameplay_report();
}

pub fn install_all() -> Result<(), String> {
    headless::install()?;
    file_isolation::install()?;
    display::install()?;
    game_session::install()?;
    hardware_init::install()?;
    frontend::install()?;
    scheme::install()?;
    config::install()?;
    weapon::install()?;
    team::install()?;
    render::install()?;
    sound::install()?;
    speech::install()?;
    input::install()?;
    ddgame_init::install()?;
    game_state_hooks::install()?;
    replay::install()?;
    task::install()?;
    weapon_release::install()?;
    Ok(())
}
