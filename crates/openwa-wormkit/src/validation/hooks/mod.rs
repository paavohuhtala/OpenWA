pub mod detour;
pub mod vtable;

use super::log_validation;

pub fn install_all() -> Result<(), String> {
    let _ = log_validation("");
    let _ = log_validation("--- Installing Validation Hooks ---");
    vtable::install()?;
    detour::install()?;
    let _ = log_validation("  All validation hooks installed.");
    Ok(())
}
