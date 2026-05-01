mod cloud;
mod filter;
mod missile;
mod try_move_position;
mod worm_handle_message;

pub fn install() -> Result<(), String> {
    cloud::install()?;
    filter::install()?;
    missile::install()?;
    try_move_position::install()?;
    worm_handle_message::install()?;
    Ok(())
}
