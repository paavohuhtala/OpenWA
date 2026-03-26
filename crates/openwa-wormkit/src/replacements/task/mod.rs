mod cloud;

pub fn install() -> Result<(), String> {
    cloud::install()?;
    Ok(())
}
