mod config;
mod frontend;
mod scheme;
mod weapon;

pub fn install_all() -> Result<(), String> {
    frontend::install()?;
    scheme::install()?;
    config::install()?;
    weapon::install()?;
    Ok(())
}
