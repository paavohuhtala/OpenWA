mod config;
mod frontend;
mod input;
mod scheme;
mod render;
mod sound;
mod speech;
mod team;
mod weapon;

pub fn install_all() -> Result<(), String> {
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
