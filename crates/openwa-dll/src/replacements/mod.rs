mod bitgrid;
mod config;
mod create_explosion;
pub(crate) mod debug_utils;
mod entity;
pub(crate) mod file_isolation;
mod fire_effect;
mod frame_hook;
mod frontend;
mod game_entity_message;
mod game_session;
mod gfx_dir;
mod hardware_init;
mod headless;
mod init_session;
mod keyboard;
mod main_loop;
mod music;
pub(crate) mod render;
mod replay;
mod replay_test;
mod scheme;
pub(crate) mod sound;
pub(crate) mod speech;
mod sprite;
mod steam;
mod string_resource;
pub(crate) mod team;
mod trace_desync;
pub(crate) mod weapon;
mod weapon_release;
mod world_init;

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
    keyboard::install()?;
    frontend::install()?;
    scheme::install()?;
    config::install()?;
    weapon::install()?;
    team::install()?;
    init_session::install()?;
    render::install()?;
    sprite::install()?;
    sound::install()?;
    speech::install()?;
    music::install()?;
    world_init::install()?;
    gfx_dir::install()?;
    replay::install()?;
    string_resource::install()?;
    entity::install()?;
    fire_effect::install()?;
    weapon_release::install()?;
    create_explosion::install()?;
    game_entity_message::install()?;
    main_loop::install()?;
    Ok(())
}
