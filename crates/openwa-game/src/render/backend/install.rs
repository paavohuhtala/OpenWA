//! Softbuffer backend installation and the `Present_Windowed` MinHook detour.
//!
//! Per call: if the gameplay gate is set and the backend is up, BitBlt the
//! handed-in framebuffer through softbuffer. Otherwise call WA's original
//! `Present_Windowed[_B]` via MinHook trampoline so DDraw keeps the
//! frontend / loading screens alive.
//!
//! Windowed mode only — softbuffer's GDI BitBlt path doesn't work under
//! fullscreen-exclusive DDraw.

use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::address::va;
use crate::rebase::rb;
use crate::render::display::context::FastcallResult;

use super::softbuffer::SoftbufferBackend;
use super::{BackendError, RenderBackend};

/// Single-threaded WA-main-thread access; `static mut` is sufficient.
static mut SOFTBUFFER: Option<SoftbufferBackend> = None;

/// One-shot gate set by `engine::main_loop::render_frame::render_frame`
/// and consumed by the present hook. When clear, the hook passes through
/// to WA's original — the frontend menu doesn't populate
/// `g_FrameBufferPtr`, so we'd present black if we ignored this.
static GAMEPLAY_RENDER_PENDING: AtomicBool = AtomicBool::new(false);

/// MinHook trampolines per variant, registered by the DLL after
/// `MinHook::create_hook`. Each detour calls back into its own variant's
/// original on the passthrough path.
static TRAMPOLINE_VARIANT_A: AtomicUsize = AtomicUsize::new(0);
static TRAMPOLINE_VARIANT_B: AtomicUsize = AtomicUsize::new(0);

#[derive(Copy, Clone, Debug)]
pub enum PresentVariant {
    /// `CompatRenderer::Present_Windowed` at `0x0059EA00`.
    A,
    /// `CompatRenderer::Present_Windowed_B` at `0x0059ED90`.
    B,
}

pub fn set_passthrough_trampoline(variant: PresentVariant, trampoline: *mut c_void) {
    let slot = match variant {
        PresentVariant::A => &TRAMPOLINE_VARIANT_A,
        PresentVariant::B => &TRAMPOLINE_VARIANT_B,
    };
    slot.store(trampoline as usize, Ordering::Release);
}

type PresentWindowedFn =
    unsafe extern "fastcall" fn(*mut c_void, *mut FastcallResult, *const u8) -> *mut FastcallResult;

pub fn mark_gameplay_render_pending() {
    GAMEPLAY_RENDER_PENDING.store(true, Ordering::Release);
}

/// Construct a [`SoftbufferBackend`] bound to the active WA window.
///
/// # Safety
/// Must be called on the WA main thread after `DisplayGfx__Init` has
/// populated `va::G_FRONTEND_HWND` and the framebuffer dims.
pub unsafe fn install_softbuffer_backend() -> Result<(), BackendError> {
    unsafe {
        // Drop any previously-constructed backend (e.g. on a Display::Init
        // retry with a fallback resolution).
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

/// MinHook detour for `CompatRenderer::Present_Windowed` (`0x0059EA00`).
///
/// # Safety
/// Detour ABI must exactly match `va::COMPAT_RENDERER_PRESENT_WINDOWED`.
pub unsafe extern "fastcall" fn softbuffer_present_replacement(
    this: *mut c_void,
    result: *mut FastcallResult,
    fb_ptr: *const u8,
) -> *mut FastcallResult {
    unsafe { present_or_passthrough(this, result, fb_ptr, &TRAMPOLINE_VARIANT_A) }
}

/// MinHook detour for `CompatRenderer::Present_Windowed_B` (`0x0059ED90`).
///
/// # Safety
/// Detour ABI must exactly match `va::COMPAT_RENDERER_PRESENT_WINDOWED_B`.
pub unsafe extern "fastcall" fn softbuffer_present_replacement_b(
    this: *mut c_void,
    result: *mut FastcallResult,
    fb_ptr: *const u8,
) -> *mut FastcallResult {
    unsafe { present_or_passthrough(this, result, fb_ptr, &TRAMPOLINE_VARIANT_B) }
}

unsafe fn present_or_passthrough(
    this: *mut c_void,
    result: *mut FastcallResult,
    fb_ptr: *const u8,
    trampoline_slot: &AtomicUsize,
) -> *mut FastcallResult {
    unsafe {
        let backend_opt: *mut SoftbufferBackend = match (&raw mut SOFTBUFFER).as_mut() {
            Some(opt) => opt.as_mut().map_or(core::ptr::null_mut(), |b| b),
            None => core::ptr::null_mut(),
        };

        let pending = GAMEPLAY_RENDER_PENDING.swap(false, Ordering::Acquire);

        if !pending || backend_opt.is_null() {
            let raw = trampoline_slot.load(Ordering::Acquire);
            if raw != 0 {
                let original: PresentWindowedFn = core::mem::transmute(raw);
                return original(this, result, fb_ptr);
            }
            // No trampoline registered (pre-DLL-init only): write success
            // so the caller's stack stays consistent on early invocations.
            if !result.is_null() {
                let success: u32 = *(rb(va::G_SUCCESS_RESULT) as *const u32);
                (*result).value = success;
            }
            return result;
        }

        let backend = &mut *backend_opt;
        let w = *(rb(va::G_FRAME_BUFFER_WIDTH) as *const u32);
        let h = *(rb(va::G_FRAME_BUFFER_HEIGHT) as *const u32);
        if !fb_ptr.is_null() && w > 0 && h > 0 {
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

        if !result.is_null() {
            let success: u32 = *(rb(va::G_SUCCESS_RESULT) as *const u32);
            (*result).value = success;
        }
        result
    }
}

/// Snapshot `DisplayGfx.palette_entries` (256 × `PALETTEENTRY`:
/// `[R, G, B, flags]`) into softbuffer's `0x00RRGGBB` u32 form.
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
