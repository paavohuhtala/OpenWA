mod config;
mod frontend;
mod scheme;
mod team;
mod weapon;

pub fn install_all() -> Result<(), String> {
    frontend::install()?;
    scheme::install()?;
    config::install()?;
    weapon::install()?;
    team::install()?;
    Ok(())
}
