//! GfxDir and GfxDirStream vtable replacements + LoadImage hook.
//!
//! Replaces the file I/O and stream vtable methods with Rust implementations,
//! and hooks GfxDir__LoadImage for any remaining WA callers.

use core::ffi::c_char;
use openwa_core::address::va;
use openwa_core::asset::gfx_dir::{
    gfx_dir_load_cached, gfx_dir_load_image, gfx_dir_read, gfx_dir_release, gfx_dir_seek,
    gfx_dir_stream_bytes_consumed, gfx_dir_stream_destructor, gfx_dir_stream_get_total_size,
    gfx_dir_stream_has_data, gfx_dir_stream_read, gfx_dir_stream_seek, GfxDir, GfxDirStream,
};
use openwa_core::log::log_line;

// ─── GfxDir__LoadImage (0x5666D0) ───────────────────────────────────────────
// Convention: usercall(ESI=gfx_dir) + 1 stack(name), RET 0x4.

extern "cdecl" fn impl_load_image(gfx_dir: *mut GfxDir, name: *const c_char) -> *mut GfxDirStream {
    unsafe { gfx_dir_load_image(gfx_dir, name) }
}

#[unsafe(naked)]
unsafe extern "C" fn load_image_trampoline() {
    core::arch::naked_asm!(
        "push edx",
        "push [esp+8]",     // name (stack param, shifted by push edx)
        "push esi",          // gfx_dir (ESI)
        "call {impl_fn}",
        "add esp, 8",
        "pop edx",
        "ret 0x4",           // callee cleans 1 stack param
        impl_fn = sym impl_load_image,
    );
}

pub fn install() -> Result<(), String> {
    use openwa_core::vtable_replace;

    vtable_replace!(openwa_core::asset::gfx_dir::GfxDirVtable, va::GFX_DIR_VTABLE, {
        read => gfx_dir_read,
        seek => gfx_dir_seek,
        load_cached => gfx_dir_load_cached,
        release => gfx_dir_release,
    })?;

    vtable_replace!(openwa_core::asset::gfx_dir::GfxDirStreamVtable, va::GFX_DIR_STREAM_VTABLE, {
        destructor => gfx_dir_stream_destructor,
        has_data => gfx_dir_stream_has_data,
        bytes_consumed => gfx_dir_stream_bytes_consumed,
        seek => gfx_dir_stream_seek,
        get_total_size => gfx_dir_stream_get_total_size,
        read => gfx_dir_stream_read,
    })?;

    unsafe {
        crate::hook::install(
            "GfxDir__LoadImage",
            va::GFX_DIR_LOAD_IMAGE,
            load_image_trampoline as *const (),
        )?;
    }

    let _ = log_line("[GfxDir] All vtable methods + LoadImage replaced");
    Ok(())
}
