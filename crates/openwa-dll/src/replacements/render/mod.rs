//! Render subsystem hooks — RenderQueue enqueue + DisplayGfx vtable patches.
//!
//! Split into two submodules:
//! - `render_queue`: RenderQueue enqueue hooks and dispatcher
//! - `display_vtable`: DisplayGfx vtable patches and stubs

mod backend;
mod display_vtable;
mod landscape;
mod render_queue;

pub fn install() -> Result<(), String> {
    render_queue::install()?;
    display_vtable::install()?;
    landscape::install()?;
    backend::install()?;
    Ok(())
}
