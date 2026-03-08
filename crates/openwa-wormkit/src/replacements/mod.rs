mod config;
mod frontend;
mod scheme;
mod render;
mod sound;
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
    Ok(())
}
