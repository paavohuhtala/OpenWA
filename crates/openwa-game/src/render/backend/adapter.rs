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

use super::RenderBackend;

/// Adapter object: lives on the heap, exposes a `CompatRenderer`-shaped
/// vtable, and forwards live slots to the wrapped `RenderBackend` impl.
#[repr(C)]
pub struct CompatBackendAdapter<B: RenderBackend> {
    /// 0x00: vtable pointer — points at the adapter's static fake vtable.
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

// Stub thunk: write the WA "success" result code, return.
//
// All the no-op slots route here. The `*result` pointer arrives in EDX
// (fastcall arg 2); we deliberately drop further stack args because they
// vary by slot and we don't read them in the stub.
unsafe extern "fastcall" fn stub_success(
    _this: *mut c_void,
    _result: *mut crate::render::display::context::FastcallResult,
) {
    // Intentionally empty — the WA success constant lives at
    // `0x008accd4`; we'll plumb it through once the live slots land.
}

impl<B: RenderBackend> CompatBackendAdapter<B> {
    /// Build an adapter around an already-constructed backend.
    ///
    /// The adapter's vtable is a static `[stub_success; 37]` for now; live
    /// slot impls land in the next change.
    pub fn new(backend: B) -> Box<Self> {
        // Static vtable storage — one per monomorphization of B is fine.
        // Wrapped in a `const` so the address is stable for the lifetime
        // of the program.
        static FAKE_VTABLE: AdapterVtable = AdapterVtable {
            // SAFETY: `stub_success` has fewer params than the slot signatures
            // declare, but fastcall callees clean up their own stack args so
            // calling it through a wider signature is safe in this direction.
            // We'll replace with typed thunks when we land live slots.
            slots: [unsafe {
                core::mem::transmute::<
                    unsafe extern "fastcall" fn(_, _),
                    unsafe extern "fastcall" fn(),
                >(stub_success)
            }; 37],
        };

        Box::new(Self {
            vtable: &FAKE_VTABLE,
            _offscreen_vtable: core::ptr::null(),
            backend,
            _marker: PhantomData,
        })
    }

    /// Borrow the wrapped backend (e.g. for diagnostics).
    pub fn backend(&self) -> &B {
        &self.backend
    }
}
