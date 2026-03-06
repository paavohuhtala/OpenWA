mod config;
mod frontend;
mod scheme;

pub fn install_all() -> Result<(), String> {
    frontend::install()?;
    scheme::install()?;
    config::install()?;
    Ok(())
}
