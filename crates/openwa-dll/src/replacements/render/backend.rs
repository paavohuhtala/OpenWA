//! Softbuffer present hook installation.
//!
//! When `OPENWA_SOFTBUFFER=1` is set, MinHooks `CompatRenderer::Flip`
//! (`0x0059DB70`) with [`openwa_game::render::backend::softbuffer_present_replacement`]
//! so per-frame DDraw flips are intercepted and the framebuffer is
//! presented through softbuffer instead.
//!
//! The actual `SoftbufferBackend` is constructed lazily by
//! `openwa_game::render::backend::install_softbuffer_backend()`, which is
//! called from `engine::hardware_init` once `DisplayGfx__Init` has
//! succeeded — see that path for HWND / framebuffer-size sourcing. Until
//! the backend is up, the replacement is a no-op success (DDraw flip
//! skipped, screen frozen).

use openwa_game::address::va;
use openwa_game::render::backend::softbuffer_present_replacement;

use crate::hook;

pub fn install() -> Result<(), String> {
    if std::env::var_os("OPENWA_SOFTBUFFER").is_none_or(|v| v != "1") {
        return Ok(());
    }
    unsafe {
        let _ = hook::install(
            "CompatRenderer::Flip (softbuffer)",
            va::COMPAT_RENDERER_FLIP,
            softbuffer_present_replacement as *const (),
        )?;
    }
    Ok(())
}
