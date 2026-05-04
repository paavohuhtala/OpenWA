//! Runtime swap-in of [`CompatBackendAdapter`] into WA's `RenderContext`.
//!
//! Call [`install_softbuffer_backend`] after WA has finished its native
//! display-init (i.e. after `DisplayGfx__Init` returns successfully and
//! `g_FrameBufferPtr` is allocated). The function:
//!
//! 1. Constructs a [`SoftbufferBackend`] bound to `g_FrontendHwnd` at the
//!    current `g_FrameBufferWidth × g_FrameBufferHeight`.
//! 2. Wraps it in [`CompatBackendAdapter`].
//! 3. Replaces the `renderer_backend` pointer at `RenderContext+0x18` with
//!    the adapter pointer (cast as `*mut c_void`).
//!
//! WA's previously-constructed `CompatRenderer` / `OpenGLCPU` is intentionally
//! leaked — its DDraw / GL resources stay live but the backend is no longer
//! reached, so the leak is bounded and only happens once at startup.

use core::ffi::c_void;

use crate::address::va;
use crate::rebase::rb;
use crate::render::display::context::RenderContext;

use super::adapter::CompatBackendAdapter;
use super::softbuffer::SoftbufferBackend;
use super::{BackendError, RenderBackend};

/// Owned global slot that keeps the leaked adapter alive (and provides a
/// hook for future teardown). `OnceLock`-style: only one install per
/// process; subsequent calls are silently ignored.
static mut INSTALLED_ADAPTER: *mut c_void = core::ptr::null_mut();

/// Install a [`SoftbufferBackend`] adapter into the active `RenderContext`.
///
/// Returns the previous backend pointer (the `CompatRenderer*` /
/// `OpenGLCPU*` WA had installed) on success, or a [`BackendError`] if
/// the softbuffer backend couldn't be constructed.
///
/// # Safety
/// Must be called once, on the WA main thread, after `DisplayGfx__Init`
/// has succeeded. The `HWND` at `va::G_FRONTEND_HWND` must be valid and
/// `g_FrameBufferWidth/Height` must be non-zero.
pub unsafe fn install_softbuffer_backend() -> Result<*mut c_void, BackendError> {
    unsafe {
        if !INSTALLED_ADAPTER.is_null() {
            return Err(BackendError::InitFailed("already installed"));
        }

        // ── 1. Read HWND + dimensions ──────────────────────────────────────
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

        // ── 2. Construct the softbuffer backend ───────────────────────────
        let backend = SoftbufferBackend::new(hwnd, width, height)?;

        // ── 3. Wrap in adapter ────────────────────────────────────────────
        let mut adapter = CompatBackendAdapter::new(backend);
        let adapter_ptr = adapter.as_renderer_ptr();
        // Leak — the adapter must outlive the process; we record the
        // pointer in `INSTALLED_ADAPTER` for future teardown.
        let _ = Box::leak(adapter);
        INSTALLED_ADAPTER = adapter_ptr;

        // ── 4. Patch RenderContext+0x18 ───────────────────────────────────
        let rc_ptr_slot = rb(va::G_RENDER_CONTEXT) as *const *mut RenderContext;
        let rc = *rc_ptr_slot;
        if rc.is_null() {
            return Err(BackendError::InitFailed("RenderContext is null"));
        }
        // Manually compute the +0x18 field address — `RenderContext` already
        // has the `renderer_backend` field at that offset; we go through a
        // raw byte ptr to keep the write explicit.
        let backend_slot = (rc as *mut u8).add(0x18) as *mut *mut c_void;
        let prior_backend = *backend_slot;
        *backend_slot = adapter_ptr;

        let _ = openwa_core::log::log_line(&format!(
            "[render-backend] softbuffer adapter installed at {:p} \
             (was {:p}); fb={}x{} hwnd={:?}",
            adapter_ptr, prior_backend, width, height, hwnd
        ));

        Ok(prior_backend)
    }
}
