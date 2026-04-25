mod bitgrid;
mod config;
mod create_explosion;
mod ddgame_init;
pub(crate) mod debug_utils;
pub(crate) mod file_isolation;
mod frame_hook;
mod frontend;
mod game_session;
mod game_state_hooks;
mod game_task_message;
mod gfx_dir;
mod hardware_init;
mod headless;
mod main_loop;
mod music;
mod render;
mod replay;
mod replay_test;
mod scheme;
mod sound;
mod speech;
mod sprite;
mod steam;
mod string_resource;
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
    steam::install()?;
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
    bitgrid::install()?;
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
    music::install()?;
    ddgame_init::install()?;
    gfx_dir::install()?;
    game_state_hooks::install()?;
    replay::install()?;
    string_resource::install()?;
    task::install()?;
    weapon_release::install()?;
    create_explosion::install()?;
    game_task_message::install()?;
    main_loop::install()?;
    Ok(())
}
