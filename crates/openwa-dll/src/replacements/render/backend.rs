//! Softbuffer present hooks. Gated on `OPENWA_SOFTBUFFER=1`.
//!
//! Hooks both slot-12 `Present_Windowed` variants because WA assigns
//! either depending on the renderer-construction path; gameplay uses
//! variant B in practice. Trampolines are stored back into the game
//! crate so the detours can pass through to the original on menu /
//! pre-match frames (see `openwa_game::render::backend`).

use openwa_game::address::va;
use openwa_game::render::backend::{
    PresentVariant, set_passthrough_trampoline, softbuffer_present_replacement,
    softbuffer_present_replacement_b,
};

use crate::hook;

pub fn install() -> Result<(), String> {
    if std::env::var_os("OPENWA_SOFTBUFFER").is_none_or(|v| v != "1") {
        return Ok(());
    }
    unsafe {
        let trampoline_a = hook::install(
            "CompatRenderer::Present_Windowed (softbuffer)",
            va::COMPAT_RENDERER_PRESENT_WINDOWED,
            softbuffer_present_replacement as *const (),
        )?;
        set_passthrough_trampoline(PresentVariant::A, trampoline_a);

        let trampoline_b = hook::install(
            "CompatRenderer::Present_Windowed_B (softbuffer)",
            va::COMPAT_RENDERER_PRESENT_WINDOWED_B,
            softbuffer_present_replacement_b as *const (),
        )?;
        set_passthrough_trampoline(PresentVariant::B, trampoline_b);
    }
    Ok(())
}
