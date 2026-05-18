//! GfxDir, GfxDirStream, and IMG decoder replacements.
//!
//! Replaces vtable methods, LoadImage, IMG_Decode, and DisplayGfx__Constructor.

use core::ffi::c_char;
use openwa_game::address::va;
use openwa_game::asset::gfx_dir::{
    GfxDir, GfxDirStream, gfx_dir_load_cached, gfx_dir_load_image, gfx_dir_read, gfx_dir_release,
    gfx_dir_seek, gfx_dir_stream_bytes_consumed, gfx_dir_stream_destructor,
    gfx_dir_stream_get_total_size, gfx_dir_stream_has_data, gfx_dir_stream_read,
    gfx_dir_stream_seek,
};
use openwa_game::asset::img::{img_decode, img_decode_cached};
use openwa_game::bitgrid::BitGrid;
use openwa_game::render::palette::PaletteContext;

// ─── GfxDir__LoadImage (0x5666D0) ───────────────────────────────────────────
// Convention: usercall(ESI=gfx_dir) + 1 stack(name), RET 0x4.

pub(crate) unsafe extern "cdecl" fn impl_load_image(
    gfx_dir: *mut GfxDir,
    name: *const c_char,
) -> *mut GfxDirStream {
    unsafe { gfx_dir_load_image(gfx_dir, name) }
}

// ─── IMG_Decode (0x4F5F80) ──────────────────────────────────────────────────
// Convention: stdcall(palette_ctx, stream, align_flag), RET 0xC.

unsafe extern "stdcall" fn img_decode_hook(
    palette_ctx: *mut PaletteContext,
    stream: *mut GfxDirStream,
    align_flag: i32,
) -> *mut BitGrid {
    unsafe {
        match img_decode(palette_ctx, stream, align_flag) {
            Some(decoded) => decoded.as_bitgrid_ptr(),
            None => core::ptr::null_mut(),
        }
    }
}

// ─── DisplayGfx__Constructor / IMG__DecodeCached (0x4F5E80) ─────────────────
// Convention: stdcall(raw_image), RET 0x4.
// PaletteContext passed implicitly via EBX (callee-saved from caller).

pub(crate) unsafe extern "cdecl" fn impl_displaygfx_ctor(
    palette_ctx: *mut PaletteContext,
    raw_image: *mut u8,
) -> *mut u8 {
    unsafe { img_decode_cached(palette_ctx, raw_image) as *mut u8 }
}

pub fn install() -> Result<(), String> {
    use openwa_game::vtable_replace;

    vtable_replace!(openwa_game::asset::gfx_dir::GfxDirVtable, va::GFX_DIR_VTABLE, {
        read => gfx_dir_read,
        seek => gfx_dir_seek,
        load_cached => gfx_dir_load_cached,
        release => gfx_dir_release,
    })?;

    vtable_replace!(openwa_game::asset::gfx_dir::GfxDirStreamVtable, va::GFX_DIR_STREAM_VTABLE, {
        destructor => gfx_dir_stream_destructor,
        has_data => gfx_dir_stream_has_data,
        bytes_consumed => gfx_dir_stream_bytes_consumed,
        seek => gfx_dir_stream_seek,
        get_total_size => gfx_dir_stream_get_total_size,
        read => gfx_dir_stream_read,
    })?;

    unsafe {
        crate::generated::hooks::install_GfxDir__LoadImage()?;

        crate::hook::install("IMG_Decode", va::IMG_DECODE, img_decode_hook as *const ())?;

        crate::generated::hooks::install_IMG__DecodeCached()?;
    }

    Ok(())
}
