//! [`SoftbufferBackend`] — first concrete [`RenderBackend`] impl.
//!
//! Wraps [`softbuffer`] to present an 8bpp paletted CPU framebuffer into
//! the existing WA `HWND`. The palette LUT (256 × RGBA8) is applied
//! inside [`present`](RenderBackend::present); softbuffer expects
//! `0x00RRGGBB` u32 pixels for its native present.
//!
//! ## Status
//!
//! Compiles standalone; not yet wired into the [`super::adapter`] vtable.
//! That happens once the live adapter slots (init / resize / flip) are
//! implemented in terms of `&mut B: RenderBackend`.

use core::ffi::c_void;
use core::num::{NonZeroIsize, NonZeroU32};
use std::rc::Rc;

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, Win32WindowHandle, WindowHandle as RwhWindowHandle, WindowsDisplayHandle,
};

use super::{BackendError, RenderBackend, WindowHandle};

/// `HasWindowHandle` / `HasDisplayHandle` adapter for a raw `HWND`. We
/// hold the handle by value (`*mut c_void`); the underlying window is
/// owned by WA and outlives the backend.
struct HwndWrapper {
    hwnd: *mut c_void,
}

// SAFETY: HWND values are inert integers; the underlying window is
// thread-safe to read for `raw-window-handle`'s purposes (softbuffer
// itself locks internally before issuing GDI calls).
unsafe impl Send for HwndWrapper {}
unsafe impl Sync for HwndWrapper {}

impl HasWindowHandle for HwndWrapper {
    fn window_handle(&self) -> Result<RwhWindowHandle<'_>, HandleError> {
        let hwnd = NonZeroIsize::new(self.hwnd as isize).ok_or(HandleError::Unavailable)?;
        let raw = Win32WindowHandle::new(hwnd);
        // SAFETY: HWND validity is upheld by WA — the window is created
        // before the backend and destroyed after.
        unsafe { Ok(RwhWindowHandle::borrow_raw(RawWindowHandle::Win32(raw))) }
    }
}

impl HasDisplayHandle for HwndWrapper {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let raw = WindowsDisplayHandle::new();
        // SAFETY: `WindowsDisplayHandle` carries no resources.
        unsafe { Ok(DisplayHandle::borrow_raw(RawDisplayHandle::Windows(raw))) }
    }
}

/// `RenderBackend` impl using [`softbuffer`].
pub struct SoftbufferBackend {
    /// `softbuffer::Surface` borrows the same `Rc<HwndWrapper>` for both
    /// its display and its window handle (Windows display handles carry
    /// no state, so the duplication is free).
    surface: softbuffer::Surface<Rc<HwndWrapper>, Rc<HwndWrapper>>,
    width: u32,
    height: u32,
    palette: [u32; 256],
}

impl RenderBackend for SoftbufferBackend {
    fn new(window: WindowHandle, width: u32, height: u32) -> Result<Self, BackendError> {
        let wrapper = Rc::new(HwndWrapper { hwnd: window });
        let context = softbuffer::Context::new(wrapper.clone())
            .map_err(|_| BackendError::InitFailed("softbuffer::Context::new"))?;
        let mut surface = softbuffer::Surface::new(&context, wrapper)
            .map_err(|_| BackendError::InitFailed("softbuffer::Surface::new"))?;
        let nz_w = NonZeroU32::new(width).ok_or(BackendError::InitFailed("zero width"))?;
        let nz_h = NonZeroU32::new(height).ok_or(BackendError::InitFailed("zero height"))?;
        surface
            .resize(nz_w, nz_h)
            .map_err(|_| BackendError::InitFailed("softbuffer resize"))?;
        Ok(Self {
            surface,
            width,
            height,
            palette: [0; 256],
        })
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), BackendError> {
        let nz_w = NonZeroU32::new(width).ok_or(BackendError::ResizeFailed("zero width"))?;
        let nz_h = NonZeroU32::new(height).ok_or(BackendError::ResizeFailed("zero height"))?;
        self.surface
            .resize(nz_w, nz_h)
            .map_err(|_| BackendError::ResizeFailed("softbuffer resize"))?;
        self.width = width;
        self.height = height;
        Ok(())
    }

    fn set_palette(&mut self, palette: &[u32; 256]) {
        self.palette.copy_from_slice(palette);
    }

    fn present(&mut self, framebuffer: &[u8]) -> Result<(), BackendError> {
        let expected = (self.width as usize) * (self.height as usize);
        if framebuffer.len() < expected {
            return Err(BackendError::PresentFailed("framebuffer too small"));
        }
        let mut buf = self
            .surface
            .buffer_mut()
            .map_err(|_| BackendError::PresentFailed("buffer_mut"))?;
        if buf.len() < expected {
            return Err(BackendError::PresentFailed("softbuffer surface too small"));
        }
        // Expand 8bpp → 32bpp through the palette LUT.
        for (dst, &idx) in buf.iter_mut().zip(framebuffer.iter()).take(expected) {
            *dst = self.palette[idx as usize];
        }
        buf.present()
            .map_err(|_| BackendError::PresentFailed("present"))?;
        Ok(())
    }

    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}
