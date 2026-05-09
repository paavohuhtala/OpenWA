mod cloud;
mod filter;
mod mine;
mod missile;
mod oil_drum;
mod try_move_position;
mod worm_handle_message;

pub fn install() -> Result<(), String> {
    cloud::install()?;
    filter::install()?;
    mine::install()?;
    missile::install()?;
    oil_drum::install()?;
    try_move_position::install()?;
    worm_handle_message::install()?;
    Ok(())
}
