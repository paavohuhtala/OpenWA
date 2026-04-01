mod config;
mod ddgame_init;
pub(crate) mod debug_utils;
mod display;
pub(crate) mod file_isolation;
mod frame_hook;
mod frontend;
mod game_session;
mod game_state_hooks;
mod hardware_init;
mod headless;
mod render;
mod replay;
mod replay_test;
mod scheme;
mod sound;
mod speech;
mod sprite;
mod task;
mod team;
mod trace_desync;
mod weapon;
mod weapon_release;

/// Write gameplay milestone report and clean up. Called from DLL_PROCESS_DETACH.
pub fn write_gameplay_report() {
    replay_test::write_gameplay_report();
    trace_desync::flush();
    file_isolation::cleanup();
}

pub fn install_all() -> Result<(), String> {
    // Infrastructure hooks — always installed
    headless::install()?;
    file_isolation::install()?;
    frame_hook::install()?;
    trace_desync::install()?;

    // Baseline mode: skip all gameplay hooks for a "nearly vanilla" reference run.
    // File isolation handles playback.thm redirection, so replay hooks aren't needed.
    if std::env::var("OPENWA_TRACE_BASELINE").is_ok() {
        return Ok(());
    }

    // Normal mode: install all hooks
    replay_test::install()?;
    display::install()?;
    game_session::install()?;
    hardware_init::install()?;
    frontend::install()?;
    scheme::install()?;
    config::install()?;
    weapon::install()?;
    team::install()?;
    render::install()?;
    sprite::install()?;
    sound::install()?;
    speech::install()?;
    ddgame_init::install()?;
    game_state_hooks::install()?;
    replay::install()?;
    task::install()?;
    weapon_release::install()?;
    Ok(())
}
