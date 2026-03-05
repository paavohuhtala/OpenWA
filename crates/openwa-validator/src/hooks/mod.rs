pub mod vtable;
pub mod detour;

use crate::log_line;

/// Install all hooks. Called from run_validation() after rebase init.
pub fn install_all() -> Result<(), String> {
    let _ = log_line("");
    let _ = log_line("--- Installing Hooks ---");

    // VTable hooks (pure Rust, no dependencies)
    vtable::install()?;

    // Inline hooks via MinHook (for constructors and free functions)
    detour::install()?;

    let _ = log_line("  All hooks installed.");
    Ok(())
}
