//! `DisplayGfx::Destructor` (slot 0) — pure-Rust port.
//!
//! Mirrors the original `DisplayGfx::DestructorImpl` (0x56A010) one-to-one.
//! See the disassembly in that function for the canonical step layout; the
//! commit message and the per-step doc comments below summarize the
//! reasoning. The slot 0 thunk lives in `vtable.rs`.
//!
//! # Steps (matching the disasm in 0x56A010)
//!
//! 1. Rebind `(*this).vtable` to the most-derived `DisplayGfxVtable` —
//!    standard MSVC C++ destructor pattern. The parent destructor will
//!    rebind to `DisplayBaseVtable` after we chain to it.
//! 2. If `display_initialized != 0`:
//!    1. `FreeLayerSpriteTable(this)` — free all 1024 [`LayerSprite`]
//!       slots in `sprite_table[1..0x3FF]`.
//!    2. Free `tile_bitmap_sets[1]` (the only used slot) plus its
//!       backing buffer.
//!    3. For every non-null entry in the CBitmap vec, call
//!       `surface.vtable[6]` (release) on its surface, then
//!       `bitmap.vtable[0](1)` (scalar deleting destructor).
//!    4. Call the layer destructor (`vtable[3](1)`) on `layer_2`,
//!       `layer_1`, `layer_0` — in that order, mirroring MSVC's
//!       reverse-construction destruction order.
//!    5. For every non-null entry in the FramePostProcessHook vec,
//!       call its destructor `vtable[0](1)`.
//!    6. Call `g_RenderContext->vtable[7](1)` (release_frame_buffer)
//!       — the renderer-backend release.
//! 3. Always-run cleanup (also taken when `display_initialized == 0`):
//!    - Free the FramePostProcessHook vec data buffer + zero its
//!      proxy/first/last/end fields.
//!    - Rebind the embedded `BitGrid` at `+0x3DA8` to the base BitGrid
//!      vtable, then free its data buffer if `external_buffer == 0`.
//!    - Free the CBitmap vec data buffer + zero its three pointers.
//!    - Chain to `DisplayBase::Destructor` (still WA-side; bridged).
//!
//! The original wraps the body in a C++ SEH frame because step 6 (the
//! parent destructor) might unwind. We don't need to mirror that — none
//! of our cleanup can throw, and Rust panics aren't C++ exceptions.

use core::ffi::c_void;
use core::ptr;

use crate::address::va;
use crate::bitgrid::{BitGridBaseVtable, BIT_GRID_BASE_VTABLE};
use crate::rebase::rb;
use crate::render::display::context::{FastcallResult, RenderContext};
use crate::render::display::frame_hook::FramePostProcessHook;
use crate::render::display::gfx::{DisplayGfx, TileBitmapSet, DISPLAY_BASE_DESTRUCTOR_IMPL};
use crate::render::display::vtable::{DisplayGfxVtable, DISPLAY_GFX_VTABLE};
use crate::render::sprite::sprite::{CBitmap, LayerSprite, LayerSpriteFrame};
use crate::wa_alloc::wa_free;

// =============================================================================
// Slot 0 thunk — the actual vtable[0] entry point.
// =============================================================================

/// `DisplayGfx::Destructor` (vtable[0] thunk, original at 0x569CE0).
///
/// Standard MSVC scalar deleting destructor: runs the cleanup body, then
/// calls `_free(this)` if `flags & 1` is set (i.e. the C++ caller used
/// `delete this`). Returns `this` per the MSVC ABI.
#[unsafe(no_mangle)]
pub unsafe extern "thiscall" fn display_gfx_destructor(
    this: *mut DisplayGfx,
    flags: u32,
) -> *mut DisplayGfx {
    display_gfx_destructor_impl(this);
    if flags & 1 != 0 {
        wa_free(this);
    }
    this
}

// =============================================================================
// Body — the actual cleanup logic.
// =============================================================================

unsafe fn display_gfx_destructor_impl(this: *mut DisplayGfx) {
    // Step 1: rebind the vtable to the most-derived DisplayGfx vtable.
    // Original at 0x56A030: `MOV [ESI], 0x66A218`. The parent destructor
    // (DisplayBase::Destructor) will overwrite it with the DisplayBase
    // vtable when we chain to it at the end.
    (*this).base.vtable = rb(DISPLAY_GFX_VTABLE) as *const DisplayGfxVtable;

    // Step 2: only run if Init() actually completed.
    if (*this).base.display_initialized != 0 {
        // Step 2a: free the LayerSprite[1024] table at +0x3DD4.
        free_layer_sprite_table(this);

        // Step 2b: free tile_bitmap_sets[1] and its inner pointer table.
        let tile_set = (*this).tile_bitmap_sets[1] as *mut TileBitmapSet;
        if !tile_set.is_null() {
            tile_bitmap_set_destructor(tile_set);
            wa_free(tile_set);
            (*this).tile_bitmap_sets[1] = ptr::null_mut();
        }

        // Step 2c: iterate the CBitmap vec, release surface + destroy bitmap.
        free_cbitmap_vec(this);

        // Step 2d: destroy the three render-context layer BitGrids in
        // reverse-construction order. Each call is `vtable[3](this, 1)`.
        for layer in [(*this).layer_2, (*this).layer_1, (*this).layer_0] {
            if !layer.is_null() {
                ((*(*layer).vtable).destructor)(layer, 1);
            }
        }

        // Step 2e: destroy every FramePostProcessHook in the vec.
        let mut p = (*this).hook_vec_first as *mut *mut FramePostProcessHook;
        let end = (*this).hook_vec_last as *mut *mut FramePostProcessHook;
        while p != end && !p.is_null() {
            let hook = *p;
            if !hook.is_null() {
                ((*(*hook).vtable).destructor)(hook, 1);
            }
            p = p.add(1);
        }

        // Step 2f: release the renderer backend's framebuffer.
        // Original at 0x56A1D6: `g_RenderContext->vtable[7](1)`. The
        // typed binding is `release_frame_buffer(this, &result, 1)`.
        let render_ctx = *(rb(va::G_RENDER_CONTEXT) as *const *mut RenderContext);
        if !render_ctx.is_null() {
            let mut result = FastcallResult::default();
            ((*(*render_ctx).vtable).release_frame_buffer)(render_ctx, &mut result, 1);
        }
    }

    // Step 3: free the hook vec backing buffer + clear pointers.
    let hook_first = (*this).hook_vec_first;
    if !hook_first.is_null() {
        wa_free(hook_first);
    }
    (*this).hook_vec_first = ptr::null_mut();
    (*this).hook_vec_last = ptr::null_mut();
    (*this).hook_vec_end = ptr::null_mut();

    // Step 4: rebind the embedded BitGrid at +0x3DA8 to the base BitGrid
    // vtable, then conditionally free its data buffer.
    let bg = &raw mut (*this).embedded_bitgrid;
    (*bg).vtable = rb(BIT_GRID_BASE_VTABLE) as *const BitGridBaseVtable;
    if (*bg).external_buffer == 0 {
        // Note: the original frees `data` unconditionally inside the
        // `external_buffer == 0` branch, even if `data` happens to be
        // null — `_free(NULL)` is a no-op so it's safe.
        wa_free((*bg).data);
    }

    // Step 5: free the CBitmap vec backing buffer + clear pointers.
    let bitmap_ptr = (*this).bitmap_ptr;
    if !bitmap_ptr.is_null() {
        wa_free(bitmap_ptr);
    }
    (*this).bitmap_ptr = ptr::null_mut();
    (*this).bitmap_end = ptr::null_mut();
    (*this).bitmap_capacity = ptr::null_mut();

    // Step 6: chain to the parent class destructor. Still WA-side until
    // someone ports DisplayBase::Destructor too.
    let parent_dtor: unsafe extern "thiscall" fn(*mut DisplayGfx) =
        core::mem::transmute(rb(DISPLAY_BASE_DESTRUCTOR_IMPL) as usize);
    parent_dtor(this);
}

// =============================================================================
// Step 2a — `DisplayGfx::FreeLayerSpriteTable` (0x56A280)
// =============================================================================

/// Pure Rust port of `DisplayGfx::FreeLayerSpriteTable` (0x56A280).
///
/// Iterates `sprite_table[1..0x3FF]` (note: index 0 is reserved/unused).
/// For each non-null `LayerSprite`:
/// 1. If the sprite has a `frame_array`, run `LayerSpriteFrame::Destructor`
///    on every element via the standard MSVC count-prefixed array layout
///    (`count` lives at `frame_array[-4]`), then free the count-prefixed
///    block.
/// 2. Free the `LayerSprite` itself.
/// 3. Zero the slot.
unsafe fn free_layer_sprite_table(this: *mut DisplayGfx) {
    for idx in 1usize..0x400 {
        let slot = &raw mut (*this).sprite_table[idx];
        let layer_sprite: *mut LayerSprite = *slot;
        if layer_sprite.is_null() {
            continue;
        }

        let frame_array = (*layer_sprite).frame_array;
        if !frame_array.is_null() {
            // Count-prefix lives at `frame_array[-4]`. The base of the
            // freeable allocation is `frame_array - 4` bytes.
            let count_ptr = (frame_array as *mut u8).sub(4) as *mut u32;
            let count = *count_ptr as usize;
            for i in 0..count {
                layer_sprite_frame_destructor(frame_array.add(i));
            }
            wa_free(count_ptr);
        }

        wa_free(layer_sprite);
        *slot = ptr::null_mut();
    }
}

/// Pure Rust port of `LayerSpriteFrame::Destructor` (0x5732E0).
///
/// Rebinds the embedded CBitmap vtable to the standard CBitmap vtable
/// (`0x643F64`) — necessary for any post-destruction code that might
/// dispatch through it — then releases the surface via its `vtable[0](1)`
/// scalar deleting destructor if non-null.
unsafe fn layer_sprite_frame_destructor(frame: *mut LayerSpriteFrame) {
    use crate::render::sprite::sprite::CBITMAP_VTABLE_MAYBE;

    (*frame).bitmap_vtable = rb(CBITMAP_VTABLE_MAYBE) as *const c_void;
    let surface = (*frame).surface;
    if !surface.is_null() {
        // surface->vtable[0](this, 1) — scalar deleting destructor.
        // Surface's vtable doesn't yet expose the destructor as a typed
        // slot (slots 3-7 are typed; slot 0 is class-specific dtor).
        let vtable = (*surface).vtable as *const usize;
        let dtor: unsafe extern "thiscall" fn(*mut c_void, u32) -> *mut c_void =
            core::mem::transmute(*vtable);
        dtor(surface as *mut c_void, 1);
    }
}

// =============================================================================
// Step 2b — `TileBitmapSet::Destructor` (0x569BC0)
// =============================================================================

/// Pure Rust port of `TileBitmapSet::Destructor` (0x569BC0).
///
/// The original takes `this` in EDI (usercall) — we sidestep the calling
/// convention entirely by porting the body. Iterates
/// `bitmap_ptrs[0..count]`, calls `vtable[0](1)` on each non-null
/// `CBitmap*`, then frees the `bitmap_ptrs` array.
unsafe fn tile_bitmap_set_destructor(set: *mut TileBitmapSet) {
    let count = (*set).count as i32;
    let bitmap_ptrs = (*set).bitmap_ptrs as *mut *mut CBitmap;
    if bitmap_ptrs.is_null() || count <= 0 {
        // Defensive: original asm always reads bitmap_ptrs unconditionally
        // but we guard the (rare) null case. Still free below if non-null.
    } else {
        for i in 0..count as usize {
            let bitmap = *bitmap_ptrs.add(i);
            if !bitmap.is_null() {
                cbitmap_vtable0(bitmap, 1);
            }
        }
    }
    if !bitmap_ptrs.is_null() {
        wa_free(bitmap_ptrs);
    }
}

// =============================================================================
// Step 2c — CBitmap vec teardown
// =============================================================================

/// Iterate the `CBitmap*` vec at `+0x3580` (start) / `+0x3584` (end) and
/// destroy every entry: release the backend surface (slot 6) + run the
/// scalar deleting destructor (slot 0) on the `CBitmap` itself.
unsafe fn free_cbitmap_vec(this: *mut DisplayGfx) {
    let start = (*this).bitmap_ptr;
    let end = (*this).bitmap_end;
    if start.is_null() || end <= start {
        return;
    }
    let count = end.offset_from(start) as usize;
    for i in 0..count {
        let bitmap = *start.add(i);
        if bitmap.is_null() {
            continue;
        }
        let surface = (*bitmap).surface;
        if !surface.is_null() {
            // surface->vtable[6](this, &result) — release the backend
            // surface storage. Result code is unused.
            let mut result = FastcallResult::default();
            ((*(*surface).vtable).release)(surface, &mut result);
        }
        cbitmap_vtable0(bitmap, 1);
    }
}

/// Call `CBitmap.vtable[0]` (the scalar deleting destructor) — `CBitmap`'s
/// vtable isn't yet typed in Rust, so we dispatch via a raw transmute. The
/// vtable is `0x643F64` for the standard tile-cache CBitmap and the
/// destructor returns `*mut CBitmap` per the MSVC ABI.
unsafe fn cbitmap_vtable0(bitmap: *mut CBitmap, flags: u32) {
    let vtable = (*bitmap).vtable as *const usize;
    let dtor: unsafe extern "thiscall" fn(*mut CBitmap, u32) -> *mut CBitmap =
        core::mem::transmute(*vtable);
    dtor(bitmap, flags);
}
