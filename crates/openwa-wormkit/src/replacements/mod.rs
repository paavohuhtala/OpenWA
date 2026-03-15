mod config;
mod display;
mod frontend;
mod game_session;
mod hardware_init;
mod headless;
pub(crate) mod input;
mod scheme;
mod render;
mod sound;
mod speech;
mod team;
mod weapon;

pub fn install_all() -> Result<(), String> {
    headless::install()?;
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
    Ok(())
}
