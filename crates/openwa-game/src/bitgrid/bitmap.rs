//! Wrappers around WA's `BitmapImage__*` family at 0x004F64C0..0x004F68A0.
//!
//! These are utility helpers that take a `DisplayBitGrid` (8bpp) and either
//! manipulate its data buffer directly ([`clear`]) or dispatch through the
//! vtable after pre-clipping ([`fill`]). The dither / scale-down pass and
//! the underlying 9-arg `BLIT_SPRITE_RECT` primitive are still bridged
//! back to WA — neither is exercised by current Rust callers.
//!
//! Used by [`crate::render::textbox::set_text`].

use openwa_core::fixed::Fixed;

use crate::bitgrid::{BLIT_SPRITE_RECT, DisplayBitGrid};
use crate::rebase::rb;

crate::define_addresses! {
    /// `BitmapImage::sub_4F64C0` — usercall(ECX=bmp, EAX=w, stack=[h, lcg, alpha]),
    /// RET 0xC. Dither / scale-down pass written by the bitgrid-copy path.
    fn/Usercall BITMAP_IMAGE_DITHER = 0x004F64C0;
}

static mut DITHER_ADDR: u32 = 0;
static mut BLIT_SPRITE_RECT_ADDR: u32 = 0;

/// Initialize bridge target addresses. Called from
/// `dispatch_frame::init_dispatch_addrs` at DLL load.
pub unsafe fn init_addrs() {
    unsafe {
        DITHER_ADDR = rb(BITMAP_IMAGE_DITHER);
        BLIT_SPRITE_RECT_ADDR = rb(BLIT_SPRITE_RECT);
    }
}

/// Memset-clear the entire data buffer of a `DisplayBitGrid`.
///
/// Equivalent to WA's `BitmapImage__clear` (0x004F65C0): per-row dword
/// fill of `data` to 0. Because `row_stride` is dword-aligned by
/// [`crate::bitgrid::BitGrid::init`] the row-by-row form is identical
/// to a single `memset(data, 0, height * row_stride)`.
pub unsafe fn clear(bmp: *mut DisplayBitGrid) {
    unsafe {
        if bmp.is_null() || (*bmp).data.is_null() {
            return;
        }
        let total = (*bmp).height as usize * (*bmp).row_stride as usize;
        core::ptr::write_bytes((*bmp).data, 0, total);
    }
}

/// Fill the rectangle `(0, 0)..(max_w, max_h)` of a `DisplayBitGrid` with
/// `color`.
///
/// Equivalent to WA's `BitmapImage__fill` (0x004F66E0): inner clipping
/// against the bitgrid's clip rect happens inside the vtable `fill_rect`
/// (slot 0 of `BitGridDisplayVtable`).
pub unsafe fn fill(bmp: *mut DisplayBitGrid, max_w: i32, max_h: i32, color: u8) {
    unsafe {
        DisplayBitGrid::fill_rect_raw(bmp, 0, 0, max_w, max_h, color);
    }
}

/// Bridge for `BitmapImage__sub_4F64C0` (0x004F64C0) — usercall:
/// `ECX=bmp`, `EAX=w`, `[stack]=(h, lcg, alpha)`, RET 0xC.
///
/// Dither / scale-down pass. Not reached from Rust today (current
/// callers always pass `Fixed::ONE`); kept for hookability of WA-side
/// callers.
#[unsafe(naked)]
pub unsafe extern "stdcall" fn dither(
    _bmp: *mut DisplayBitGrid,
    _w: i32,
    _h: i32,
    _lcg: u32,
    _alpha: Fixed,
) {
    core::arch::naked_asm!(
        "push ecx",
        "mov ecx, dword ptr [esp+0x8]",     // bmp -> ECX
        "mov eax, dword ptr [esp+0xC]",     // w   -> EAX
        "push dword ptr [esp+0x18]",        // alpha
        "push dword ptr [esp+0x18]",        // lcg
        "push dword ptr [esp+0x18]",        // h
        "call dword ptr [{addr}]",
        "pop ecx",
        "ret 0x14",
        addr = sym DITHER_ADDR,
    );
}

/// Bridge for WA's `BitGrid::BlitSpriteRect` (0x004F6910) — usercall:
/// `ESI=dst`, 9 stack params, RET 0x24.
///
/// Distinct from [`crate::render::display::sprite_blit::blit_sprite_rect`]
/// (the higher-level Rust port with `PixelGridMut` / `BlitSource`
/// wrappers): this one matches WA's raw register convention and is
/// invoked by the textbox dither path which holds raw `DisplayBitGrid`
/// pointers.
#[unsafe(naked)]
pub unsafe extern "stdcall" fn blit_sprite_rect_raw(
    _arg1: i32,
    _arg2: i32,
    _w: i32,
    _h: i32,
    _src: *mut DisplayBitGrid,
    _arg6: i32,
    _arg7: i32,
    _arg8: i32,
    _arg9: i32,
    _dst: *mut DisplayBitGrid,
) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, dword ptr [esp+0x2C]",    // dst (last arg) -> ESI
        // Forward 9 stack params verbatim (deepest = arg9 pushed first).
        "push dword ptr [esp+0x28]",        // arg9
        "push dword ptr [esp+0x28]",        // arg8
        "push dword ptr [esp+0x28]",        // arg7
        "push dword ptr [esp+0x28]",        // arg6
        "push dword ptr [esp+0x28]",        // src
        "push dword ptr [esp+0x28]",        // h
        "push dword ptr [esp+0x28]",        // w
        "push dword ptr [esp+0x28]",        // arg2
        "push dword ptr [esp+0x28]",        // arg1
        "call dword ptr [{addr}]",
        "pop esi",
        "ret 0x28",
        addr = sym BLIT_SPRITE_RECT_ADDR,
    );
}
