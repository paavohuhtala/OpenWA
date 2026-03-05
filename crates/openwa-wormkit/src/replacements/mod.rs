mod frontend;

pub fn install_all() -> Result<(), String> {
    frontend::install()?;
    Ok(())
}
