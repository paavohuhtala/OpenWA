mod cloud;
mod filter;
mod missile;
mod try_move_position;

pub fn install() -> Result<(), String> {
    cloud::install()?;
    filter::install()?;
    missile::install()?;
    try_move_position::install()?;
    Ok(())
}
