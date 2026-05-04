//! `RenderBackend` — Rust-side rendering backend abstraction for OpenWA.
//!
//! Replaces WA's `CompatRenderer` / `OpenGLCPU` dispatch surface with a small
//! trait that softbuffer / SDL / wgpu impls can satisfy. The runtime adapter
//! ([`adapter::CompatBackendAdapter`]) wraps any `RenderBackend` impl with
//! the 37-slot `CompatRendererVtable` shape so existing `RenderContext` /
//! `DisplayGfx` call sites dispatch into Rust unchanged. We vtable-patch
//! the `renderer_backend` pointer at `RenderContext+0x18` to point at the
//! adapter object instead of letting WA construct a `CompatRenderer`.
//!
//! ## Why this trait shape
//!
//! Two architectural facts (audited 2026-05-04 — see
//! `project_render_backend_dispatch_audit.md`):
//!
//! 1. The "primary surface" the draw code interacts with is `g_FrameBufferPtr`,
//!    a plain `wa_malloc`'d 8bpp paletted CPU buffer. There is no separate
//!    VRAM-backed primary that needs locking. `RenderContext::lock_surface_*`
//!    just returns `g_FrameBufferPtr` without dispatching to the backend.
//!
//! 2. The only backend operation actually reached on the modern call paths is
//!    **`flip`** (CompatRenderer slot 13) — its job is to make the current
//!    contents of `g_FrameBufferPtr` visible on screen. Everything else
//!    (`fill_rect`, sprite blits, draws) writes to the framebuffer directly.
//!
//! So the trait collapses to: bind to a window, resize, set palette, present
//! the framebuffer, report dimensions. No lock/unlock semantics.
//!
//! ## Implementation status
//!
//! Trait + stub adapter only. The adapter's vtable thunks are no-op success
//! returns; it isn't wired into the runtime yet. Next step is a softbuffer
//! impl + the runtime swap-in (intercept the `CompatRenderer` constructor
//! call site in `Frontend__MainNavigationLoop`).

pub mod adapter;
#[cfg(target_os = "windows")]
pub mod softbuffer;

use core::ffi::c_void;

/// Opaque window handle. Currently always an `HWND` on Win32; abstracted as
/// `*mut c_void` so the trait stays platform-neutral as we move toward a
/// truly portable target.
pub type WindowHandle = *mut c_void;

/// Errors a `RenderBackend` impl can produce. Stub variant for now;
/// concrete impls will likely want richer enums.
#[derive(Debug)]
pub enum BackendError {
    /// Initialization failed (window binding, surface creation, etc.).
    InitFailed(&'static str),
    /// Resize failed.
    ResizeFailed(&'static str),
    /// Present failed.
    PresentFailed(&'static str),
}

/// Software-rendering backend trait.
///
/// Implementations make the contents of an 8bpp paletted CPU framebuffer
/// visible on screen. The framebuffer itself is owned by `RenderContext`
/// (i.e. the global `g_FrameBufferPtr`); the backend never sees it between
/// `present` calls.
pub trait RenderBackend: Sized {
    /// Bind to an existing native window with the given initial size.
    fn new(window: WindowHandle, width: u32, height: u32) -> Result<Self, BackendError>;

    /// Window-resize / display-mode-change hook.
    fn resize(&mut self, width: u32, height: u32) -> Result<(), BackendError>;

    /// Update the active 256-entry palette. Each entry is RGBA8 packed
    /// little-endian (`0x00BBGGRR` on x86); takes effect on the next
    /// [`present`](Self::present).
    fn set_palette(&mut self, palette: &[u32; 256]);

    /// Make the contents of `framebuffer` visible on screen.
    /// `framebuffer.len()` must equal `width * height` (stride = width,
    /// no padding); each byte is a palette index.
    fn present(&mut self, framebuffer: &[u8]) -> Result<(), BackendError>;

    /// Current display dimensions.
    fn dimensions(&self) -> (u32, u32);
}
