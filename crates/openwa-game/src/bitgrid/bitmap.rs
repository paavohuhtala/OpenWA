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

use crate::bitgrid::DisplayBitGrid;
use crate::generated::wa_calls;

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
#[inline]
pub unsafe fn dither(bmp: *mut DisplayBitGrid, w: i32, h: i32, lcg: u32, alpha: Fixed) {
    unsafe {
        wa_calls::BitmapImage::sub_4F64C0(bmp, w, h, lcg, alpha);
    }
}

/// Bridge for WA's `BitGrid::BlitSpriteRect` (0x004F6910) — usercall:
/// `ESI=dst`, 9 stack params, RET 0x24.
///
/// Distinct from [`crate::render::display::sprite_blit::blit_sprite_rect`]
/// (the higher-level Rust port with `PixelGridMut` / `BlitSource`
/// wrappers): this one matches WA's raw register convention and is
/// invoked by the textbox dither path which holds raw `DisplayBitGrid`
/// pointers.
#[inline]
#[allow(clippy::too_many_arguments)]
pub unsafe fn blit_sprite_rect_raw(
    dst_x: i32,
    dst_y: i32,
    w: i32,
    h: i32,
    src: *mut DisplayBitGrid,
    src_x: i32,
    src_y: i32,
    color_table: *const u8,
    flags: u32,
    dst: *mut DisplayBitGrid,
) {
    unsafe {
        wa_calls::BitGrid::BlitSpriteRect(
            dst,
            dst_x,
            dst_y,
            w,
            h,
            src,
            src_x,
            src_y,
            color_table,
            flags,
        );
    }
}
