//! `CompatBackendAdapter` — wraps a [`RenderBackend`] impl with a fake
//! `CompatRendererVtable` so existing WA call sites (RC thunks, direct
//! dispatch) reach our Rust impl unchanged.
//!
//! ## Layout
//!
//! The adapter object mimics the `CompatRenderer` struct's first two fields
//! (vtable pointer at +0x00, null offscreen pointer at +0x04 — see audit in
//! `compat_renderer.rs`) so `RenderContext+0x18` can be vtable-patched to
//! point at it. After those, we own the layout entirely.
//!
//! ## Live-vs-stub vtable slots
//!
//! Per the audit (`project_render_backend_dispatch_audit.md`), only a small
//! subset of the 37 backend slots is ever reached on modern call paths. The
//! adapter implements those for real; the rest return the WA success
//! constant and do nothing.
//!
//! | Slot | Status | Adapter behavior |
//! |------|--------|------------------|
//! | 0    | live   | no-op (init was done at adapter ctor) |
//! | 3    | live   | report dimensions |
//! | 6    | live   | resize via trait |
//! | 7    | live   | trigger Drop (deferred — first cut just no-ops) |
//! | 9    | live   | resize via trait |
//! | 13   | live   | present via trait, sourcing the framebuffer from globals |
//! | 1, 2, 4, 5, 8, 10, 11, 14-24 | stub | success no-op |
//! | 25-36 | dead | success no-op (never reached anyway) |
//!
//! ## Status
//!
//! Stub-level. All slots are no-op success returns; the trait is wired in
//! only conceptually. Next step is to implement the live slots in terms of
//! the bound `RenderBackend` impl, then patch the runtime.

use core::ffi::c_void;
use core::marker::PhantomData;

use crate::rebase::rb;
use crate::render::display::context::FastcallResult;

use super::RenderBackend;

crate::define_addresses! {
    /// Global "success result" sentinel read by `RenderContext` and
    /// `Surface` methods. Every successful fastcall writes the value at
    /// this address into the caller's result buffer; downstream callers
    /// compare `*result == g_SuccessResult` to detect success. Only
    /// `[READ]` xrefs are visible in the binary — the value appears to
    /// be statically zero, but reading it dynamically is robust against
    /// any unknown initialization path.
    global G_SUCCESS_RESULT = 0x008ACCD4;

    /// Software framebuffer pointer (`wa_malloc(width * height)`, 8bpp
    /// paletted). Allocated by `RC::ConstructFrameBuffer`
    /// (`0x005A2430`); written to by every CPU-side draw op
    /// (sprite blit, fill_rect, draw_landscape) and read by the backend
    /// `flip` slot to upload to screen.
    global G_FRAME_BUFFER_PTR = 0x007A0EEC;
    /// Software framebuffer width in pixels. Matches DisplayGfx's display
    /// dimensions when not letterboxed.
    global G_FRAME_BUFFER_WIDTH = 0x007A0EF0;
    /// Software framebuffer height in pixels.
    global G_FRAME_BUFFER_HEIGHT = 0x007A0EF4;
}

/// Adapter object: lives on the heap, exposes a `CompatRenderer`-shaped
/// vtable, and forwards live slots to the wrapped `RenderBackend` impl.
#[repr(C)]
pub struct CompatBackendAdapter<B: RenderBackend> {
    /// 0x00: vtable pointer — points at the per-`B` static fake vtable.
    vtable: *const AdapterVtable,
    /// 0x04: matches the unused `offscreen_vtable` field on the real
    /// `CompatRenderer`; always null.
    _offscreen_vtable: *const c_void,
    /// The wrapped backend. Adapter owns it.
    backend: B,
    _marker: PhantomData<B>,
}

/// Bare 37-slot vtable shaped like `CompatRendererVtable`. We don't reuse
/// the `#[vtable]`-generated type because that one's slot signatures are
/// authored for the WA-side `CompatRenderer` impl; here we just need a
/// flat array of fastcall thunks. All slots have the same shape from the
/// adapter's POV: `(this, *result, ...args)` returning `*result`.
#[repr(C)]
struct AdapterVtable {
    slots: [unsafe extern "fastcall" fn(); 37],
}

// SAFETY: `AdapterVtable` is a POD array of function pointers — Send + Sync
// trivially, but we have to assert it manually because raw fn pointers
// aren't `Send`/`Sync` by default in older Rust editions. (Rust 1.78+ they
// are; the bound is left in for clarity.)
unsafe impl Sync for AdapterVtable {}

/// Erase the param list of a fastcall fn so it fits in the
/// `[fn(); 37]` array. Fastcall callees clean their own stack args, so
/// calling a narrower-typed thunk through a wider-typed pointer is safe
/// in this direction (callee sees the args it expects; the caller's
/// extra stack args get cleaned by the callee anyway since we route slot
/// arity through the WA-side vtable definition).
macro_rules! erase {
    ($fn_:expr) => {{
        let f: unsafe extern "fastcall" fn() = unsafe { core::mem::transmute($fn_ as *const ()) };
        f
    }};
}

// ─── Slot thunks ────────────────────────────────────────────────────────────

/// Write the WA success constant into the result buffer.
unsafe fn write_success(result: *mut FastcallResult) {
    if result.is_null() {
        return;
    }
    unsafe {
        let success: u32 = *(rb(G_SUCCESS_RESULT) as *const u32);
        (*result).value = success;
    }
}

/// Stub thunk for slots we don't implement — just signals success.
unsafe extern "fastcall" fn stub_success(_this: *mut c_void, result: *mut FastcallResult) {
    unsafe { write_success(result) };
}

/// Slot 3 — `get_display_size(out_w, out_h)`.
unsafe extern "fastcall" fn get_display_size_thunk<B: RenderBackend>(
    this: *mut CompatBackendAdapter<B>,
    result: *mut FastcallResult,
    out_w: *mut u32,
    out_h: *mut u32,
) {
    unsafe {
        if !this.is_null() {
            let (w, h) = (*this).backend.dimensions();
            if !out_w.is_null() {
                *out_w = w;
            }
            if !out_h.is_null() {
                *out_h = h;
            }
        }
        write_success(result);
    }
}

/// Slot 6 — `set_display_mode(width, height)`. Routes to `B::resize`. Also
/// reached via slot 9 (`set_display_dims`) which is a thin wrapper.
unsafe extern "fastcall" fn set_display_mode_thunk<B: RenderBackend>(
    this: *mut CompatBackendAdapter<B>,
    result: *mut FastcallResult,
    width: i32,
    height: i32,
) {
    unsafe {
        if !this.is_null() && width > 0 && height > 0 {
            let _ = (*this).backend.resize(width as u32, height as u32);
        }
        write_success(result);
    }
}

/// Slot 13 — `flip`. Pulls the current 8bpp framebuffer slice + the
/// active palette and pushes both to the backend, then presents.
unsafe extern "fastcall" fn flip_thunk<B: RenderBackend>(
    this: *mut CompatBackendAdapter<B>,
    result: *mut FastcallResult,
) {
    unsafe {
        if !this.is_null() {
            let fb_ptr = *(rb(G_FRAME_BUFFER_PTR) as *const *const u8);
            let w = *(rb(G_FRAME_BUFFER_WIDTH) as *const u32);
            let h = *(rb(G_FRAME_BUFFER_HEIGHT) as *const u32);
            if !fb_ptr.is_null() && w > 0 && h > 0 {
                let len = (w as usize).saturating_mul(h as usize);
                let fb = core::slice::from_raw_parts(fb_ptr, len);

                let palette = read_active_palette();
                (*this).backend.set_palette(&palette);

                let _ = (*this).backend.present(fb);
            }
        }
        write_success(result);
    }
}

/// Read the current 256-entry palette from `DisplayGfx.palette_entries`
/// and convert it into softbuffer-compatible `0x00RRGGBB` u32 entries.
///
/// The DisplayGfx palette layout is 256 × 4 bytes in `LOGPALETTE` /
/// `PALETTEENTRY` form: `[peRed, peGreen, peBlue, peFlags]` per entry.
/// We pack the RGB triple into a u32 ignoring the flags byte.
unsafe fn read_active_palette() -> [u32; 256] {
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
        // `palette_entries` actually starts at +0x358D (off-by-one byte from
        // the struct comment); the field declaration covers 0x358D..0x398D.
        // Read 256 × 4 bytes from there.
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

// ─── Vtable assembly ────────────────────────────────────────────────────────

impl<B: RenderBackend> CompatBackendAdapter<B> {
    /// Build the per-`B` static vtable, returning a stable pointer. Each
    /// monomorphization of `B` gets its own `OnceLock`-backed storage; the
    /// returned pointer is valid for the program lifetime.
    fn fake_vtable() -> *const AdapterVtable {
        use std::sync::OnceLock;
        static_assert_size_eq();
        // Per-monomorphization static.
        static VTABLE_CELL: OnceLock<AdapterVtable> = OnceLock::new();
        VTABLE_CELL.get_or_init(|| AdapterVtable {
            slots: build_slot_table::<B>(),
        })
    }

    /// Build an adapter around an already-constructed backend.
    pub fn new(backend: B) -> Box<Self> {
        Box::new(Self {
            vtable: Self::fake_vtable(),
            _offscreen_vtable: core::ptr::null(),
            backend,
            _marker: PhantomData,
        })
    }

    /// Borrow the wrapped backend (e.g. for diagnostics).
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Cast as a raw `*mut c_void` — used when writing the adapter pointer
    /// into `RenderContext+0x18`.
    pub fn as_renderer_ptr(self: &mut Box<Self>) -> *mut c_void {
        &raw mut **self as *mut c_void
    }
}

const fn static_assert_size_eq() {
    // 37 slots × 4 bytes (i686) = 148 bytes; just a guard against pointer-size
    // assumptions silently shifting.
    const _: () = assert!(core::mem::size_of::<AdapterVtable>() == 37 * 4);
}

fn build_slot_table<B: RenderBackend>() -> [unsafe extern "fastcall" fn(); 37] {
    let mut slots: [unsafe extern "fastcall" fn(); 37] = [stub_success_erased(); 37];
    slots[3] = erase!(get_display_size_thunk::<B>);
    slots[6] = erase!(set_display_mode_thunk::<B>);
    slots[9] = erase!(set_display_mode_thunk::<B>); // set_display_dims — same shape
    slots[13] = erase!(flip_thunk::<B>);
    slots
}

fn stub_success_erased() -> unsafe extern "fastcall" fn() {
    erase!(stub_success)
}
