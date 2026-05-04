//! Softbuffer backend installation.
//!
//! Two-phase install:
//!
//! 1. **Construct** ([`install_softbuffer_backend`]) — call once after WA's
//!    `DisplayGfx__Init` succeeds. Reads `g_FrontendHwnd` +
//!    `g_FrameBufferWidth`/`Height`, builds a [`SoftbufferBackend`], stores
//!    it in [`SOFTBUFFER`]. Does **not** touch any WA state.
//!
//! 2. **Hook** — the DLL crate (`openwa-dll`) installs a MinHook on
//!    `CompatRenderer__Flip` (`0x0059DB70`) pointing at
//!    [`softbuffer_present_replacement`]. The hook fires on every per-frame
//!    flip; if [`SOFTBUFFER`] is `Some`, it reads `g_FrameBufferPtr` and the
//!    DisplayGfx palette, presents through softbuffer, and writes the WA
//!    success constant into the caller's result buffer. If `SOFTBUFFER` is
//!    `None` (env var not set), the replacement falls back to a no-op
//!    success.
//!
//! Compared to the earlier "patch `RenderContext+0x18`" approach: this leaves
//! WA's CompatRenderer fully functional for everything other than the
//! per-frame DDraw `Flip`. No vtable layout assumptions; no out-of-bounds
//! slot dispatch risk.
//!
//! ## Limitations
//!
//! - Requires **windowed** display mode. softbuffer talks to the HWND via
//!   GDI (`BitBlt` on a DIB section); fullscreen-exclusive DDraw blocks
//!   GDI access to the primary surface.
//! - DDraw resources allocated by `CompatRenderer` stay live but the
//!   per-frame `Flip` never runs. The back-buffer cycle is paused; we
//!   present from `g_FrameBufferPtr` directly.

use core::ffi::c_void;

use crate::address::va;
use crate::rebase::rb;
use crate::render::display::context::FastcallResult;

use super::softbuffer::SoftbufferBackend;
use super::{BackendError, RenderBackend};

/// Singleton state set by [`install_softbuffer_backend`] and read by
/// [`softbuffer_present_replacement`]. Single-threaded access from the WA
/// main thread; `static mut` is sufficient.
static mut SOFTBUFFER: Option<SoftbufferBackend> = None;

/// Construct a [`SoftbufferBackend`] bound to the active WA window.
///
/// # Safety
/// Must be called once, on the WA main thread, **after** WA's
/// `DisplayGfx__Init` has succeeded. The HWND at `va::G_FRONTEND_HWND`
/// must be valid and `g_FrameBufferWidth`/`Height` must be non-zero.
pub unsafe fn install_softbuffer_backend() -> Result<(), BackendError> {
    unsafe {
        // Drop any previously-constructed backend before allocating a new
        // one (e.g. on a `DisplayGfx::Init` retry with a fallback resolution).
        // `core::ptr::replace` takes the old value out so it gets dropped.
        let _old = core::ptr::replace(&raw mut SOFTBUFFER, None);

        let hwnd_raw = *(rb(va::G_FRONTEND_HWND) as *const usize);
        if hwnd_raw == 0 {
            return Err(BackendError::InitFailed("HWND is null"));
        }
        let hwnd = hwnd_raw as *mut c_void;

        let width = *(rb(va::G_FRAME_BUFFER_WIDTH) as *const u32);
        let height = *(rb(va::G_FRAME_BUFFER_HEIGHT) as *const u32);
        if width == 0 || height == 0 {
            return Err(BackendError::InitFailed("zero framebuffer size"));
        }

        let backend = SoftbufferBackend::new(hwnd, width, height)?;
        SOFTBUFFER = Some(backend);

        let _ = openwa_core::log::log_line(&format!(
            "[render-backend] softbuffer constructed: fb={}x{} hwnd={:?}",
            width, height, hwnd
        ));

        Ok(())
    }
}

/// Replacement for `CompatRenderer::Flip` (`0x0059DB70`,
/// `__fastcall(this, result)`).
///
/// Wired in by the DLL via MinHook when `OPENWA_SOFTBUFFER=1` is in effect.
/// Reads `g_FrameBufferPtr` + the active 256-entry palette from
/// `DisplayGfx`, presents through softbuffer, and writes the WA success
/// constant into the caller's result buffer.
///
/// If [`SOFTBUFFER`] is `None` (e.g. early frames before construction
/// completes), this falls back to a no-op success — the DDraw flip is
/// still skipped, so the screen will simply not advance until the
/// backend is up.
///
/// # Safety
/// Called via MinHook detour. ABI must exactly match the original target.
pub unsafe extern "fastcall" fn softbuffer_present_replacement(
    _this: *mut c_void,
    result: *mut FastcallResult,
) -> *mut FastcallResult {
    unsafe {
        let backend_opt: *mut SoftbufferBackend = match (&raw mut SOFTBUFFER).as_mut() {
            Some(opt) => opt.as_mut().map_or(core::ptr::null_mut(), |b| b),
            None => core::ptr::null_mut(),
        };
        if !backend_opt.is_null() {
            let backend = &mut *backend_opt;
            let fb_ptr = *(rb(va::G_FRAME_BUFFER_PTR) as *const *const u8);
            let w = *(rb(va::G_FRAME_BUFFER_WIDTH) as *const u32);
            let h = *(rb(va::G_FRAME_BUFFER_HEIGHT) as *const u32);
            if !fb_ptr.is_null() && w > 0 && h > 0 {
                // Resize if WA changed dims since last frame.
                let (cur_w, cur_h) = backend.dimensions();
                if cur_w != w || cur_h != h {
                    let _ = backend.resize(w, h);
                }
                let len = (w as usize).saturating_mul(h as usize);
                let fb = core::slice::from_raw_parts(fb_ptr, len);

                let palette = read_active_palette();
                backend.set_palette(&palette);

                let _ = backend.present(fb);
            }
        }

        if !result.is_null() {
            let success: u32 = *(rb(va::G_SUCCESS_RESULT) as *const u32);
            (*result).value = success;
        }
        result
    }
}

/// Read the current 256-entry palette from `DisplayGfx.palette_entries`
/// and convert it into softbuffer-compatible `0x00RRGGBB` u32 entries.
///
/// The DisplayGfx palette layout is 256 × 4 bytes in `LOGPALETTE` /
/// `PALETTEENTRY` form: `[peRed, peGreen, peBlue, peFlags]` per entry.
/// We pack the RGB triple into a u32 ignoring the flags byte.
fn read_active_palette() -> [u32; 256] {
    use crate::engine::game_session::get_game_session;
    use crate::render::display::gfx::DisplayGfx;

    let mut out = [0u32; 256];
    unsafe {
        let session = get_game_session();
        if session.is_null() {
            return out;
        }
        let display = (*session).display as *const DisplayGfx;
        if display.is_null() {
            return out;
        }
        let entries = (*display).palette_entries.as_ptr();
        for (i, slot) in out.iter_mut().enumerate() {
            let base = entries.add(i * 4);
            let r = *base as u32;
            let g = *base.add(1) as u32;
            let b = *base.add(2) as u32;
            *slot = (r << 16) | (g << 8) | b;
        }
    }
    out
}
