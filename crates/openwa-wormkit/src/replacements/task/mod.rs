mod cloud;
mod filter;

pub fn install() -> Result<(), String> {
    cloud::install()?;
    filter::install()?;
    Ok(())
}
