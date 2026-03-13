mod config;
mod frontend;
mod game_session;
pub(crate) mod input;
mod scheme;
mod render;
mod sound;
mod speech;
mod team;
mod weapon;

pub fn install_all() -> Result<(), String> {
    game_session::install()?;
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
