mod cloud;
mod filter;
mod missile;

pub fn install() -> Result<(), String> {
    cloud::install()?;
    filter::install()?;
    missile::install()?;
    Ok(())
}
