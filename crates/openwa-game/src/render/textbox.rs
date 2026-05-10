//! Textbox text-rendering — Rust ports and the [`Textbox`] struct itself.
//!
//! A textbox is a pre-allocated `DisplayBitGrid` plus an inline 256-byte
//! cache of the most recent rendered text + params, allocated by
//! `DisplayGfx::ConstructTextbox` (0x004FAF00). Callers ([`set_text`])
//! pass a string in and receive a bitmap pointer + pixel dimensions
//! back; the caller is then responsible for blitting that bitmap onto
//! the target surface.

use core::ffi::c_char;

use openwa_core::fixed::Fixed;
use openwa_core::rng::wa_lcg;

use crate::FieldRegistry;
use crate::bitgrid::bitmap;
use crate::bitgrid::{BIT_GRID_DISPLAY_VTABLE, BitGrid, BitGridDisplayVtable, DisplayBitGrid};
use crate::rebase::rb;
use crate::render::display::DisplayGfx;
use crate::render::display::vtable::{draw_text_on_bitmap, get_font_metric};
use crate::wa_alloc::{wa_free, wa_malloc_struct_zeroed};

crate::define_addresses! {
    class "DisplayGfx" {
        /// `DisplayGfx::ConstructTextbox` (0x004FAF00) — `__thiscall(this =
        /// DisplayGfx, textbox_buf, anchor, kind)`, RET 0xC. Initialises a
        /// caller-supplied 0x158-byte buffer as a [`Textbox`] (allocates
        /// the primary `DisplayBitGrid`, picks the font, etc.) and returns
        /// the same pointer. ECX must point at the `DisplayGfx` because the
        /// ctor immediately dispatches `vtable[8]` on it (font metrics).
        fn/Thiscall CONSTRUCT_TEXTBOX = 0x004FAF00;
    }
}

crate::define_addresses! {
    /// `&DAT_006A9020` — flat `i32` table indexed by `font_index + style_index*10`.
    /// Each entry is the `font_id` passed to DisplayGfx vtable slot 7.
    global FONT_ID_TABLE = 0x006A9020;
}

// ─── Textbox struct ────────────────────────────────────────────────────────

/// Textbox object, allocated by `DisplayGfx::ConstructTextbox` (0x004FAF00).
/// Holds a pre-allocated `DisplayBitGrid` text canvas plus an inline cache
/// of the most recent rendered text + params, so repeat calls with the
/// same args return immediately.
#[derive(FieldRegistry)]
#[repr(C)]
pub struct Textbox {
    /// 0x000: Output width clamp (set by ctor from caller's max-width arg).
    pub max_width: i32,
    /// 0x004: Output height clamp (set by ctor from font metrics).
    pub max_height: i32,
    /// 0x008: Style row index — multiplied by 10 to stride [`FONT_ID_TABLE`].
    pub style_index: i32,
    /// 0x00C: Primary text canvas. Cleared/filled and then text-blitted.
    pub primary_bitmap: *mut DisplayBitGrid,
    /// 0x010: Lazy alpha-grid copy. Allocated on first `scale < 1.0` call.
    pub bitgrid_copy: *mut DisplayBitGrid,
    /// 0x014: Owning `DisplayGfx` — used for vtable text dispatch.
    pub display: *mut DisplayGfx,
    /// 0x018: LCG state advanced by the dither path
    /// (`x*0x19660D + 0x3C6EF35F`).
    pub lcg_state: u32,
    /// 0x01C: Inline cached text buffer (NUL-padded; compared via `strncmp`
    /// over the full 256 bytes).
    pub cached_text: [u8; 0x100],
    /// 0x11C: Cached `font_index`.
    pub cached_font_index: i32,
    /// 0x120: Cached output width.
    pub cached_width: i32,
    /// 0x124: Cached output height.
    pub cached_height: i32,
    /// 0x128: Cached `fill_color`.
    pub cached_fill_color: i32,
    /// 0x12C: Cached `border_color`.
    pub cached_border_color: i32,
    /// 0x130: Cached `scale`
    pub cached_scale: Fixed,
    /// 0x134: Bitmap pointer returned from the most recent call —
    /// `primary_bitmap` for `scale >= 1.0`, `bitgrid_copy` otherwise.
    pub return_bitmap: *mut DisplayBitGrid,
}

const _: () = assert!(core::mem::size_of::<Textbox>() == 0x138);

impl Textbox {
    /// Bridge to WA's `DisplayGfx::ConstructTextbox` (0x004FAF00). Initialises
    /// the caller-supplied 0x158-byte `buf` as a `Textbox` against the given
    /// `DisplayGfx` (allocates the primary `DisplayBitGrid`, picks the font
    /// from `kind`, etc.) and returns the same pointer cast to `*mut Textbox`.
    /// Returns null if `buf` is null.
    pub unsafe fn construct(
        display: *mut DisplayGfx,
        buf: *mut Textbox,
        anchor: i32,
        kind: i32,
    ) -> *mut Textbox {
        type Fn =
            unsafe extern "thiscall" fn(*mut DisplayGfx, *mut Textbox, i32, i32) -> *mut Textbox;
        unsafe {
            let f: Fn = core::mem::transmute(rb(CONSTRUCT_TEXTBOX) as usize);
            f(display, buf, anchor, kind)
        }
    }

    /// Tear down a `Textbox` allocated by [`Textbox::construct`]: destroy the
    /// two `DisplayBitGrid` children ([`primary_bitmap`](Self::primary_bitmap)
    /// and [`bitgrid_copy`](Self::bitgrid_copy)) via their vtable slot 3
    /// (`thiscall(this, flags=1)`) and free the textbox itself.
    pub unsafe fn destroy(this: *mut Textbox) {
        unsafe {
            if this.is_null() {
                return;
            }
            for child in [(*this).primary_bitmap, (*this).bitgrid_copy] {
                if !child.is_null() {
                    DisplayBitGrid::destructor_raw(child, 1);
                }
            }
            wa_free(this as *mut u8);
        }
    }
}

// ─── set_text ─────────────────────────────────────────────────────────────

/// Resolve `(font_index, style_index)` to the runtime `font_id` consumed by
/// DisplayGfx vtable slot 7.
unsafe fn resolve_font_id(font_index: i32, style_index: i32) -> i32 {
    unsafe {
        let table = rb(FONT_ID_TABLE) as *const i32;
        let idx = (font_index as isize).wrapping_add((style_index as isize).wrapping_mul(10));
        *table.offset(idx)
    }
}

/// Compare the textbox's cached text against `text` over the full 256-byte
/// cache slot, matching WA's `_strncmp(cache, text, 0x100)`.
unsafe fn cache_text_matches(cache: &[u8; 0x100], text: *const c_char) -> bool {
    unsafe {
        for (i, &a) in cache.iter().enumerate() {
            let b = *text.add(i) as u8;
            if a != b {
                return false;
            }
            if a == 0 {
                return true;
            }
        }
        true
    }
}

/// Copy `text` into `cache` for up to 256 bytes — `_strncpy(cache, text, 0x100)`.
/// `strncpy` zero-fills the tail after the first NUL, so reproduce that here
/// to keep `cached_text` matching WA byte-for-byte.
unsafe fn cache_text_copy(cache: &mut [u8; 0x100], text: *const c_char) {
    unsafe {
        let mut hit_nul = false;
        for (i, slot) in cache.iter_mut().enumerate() {
            if hit_nul {
                *slot = 0;
                continue;
            }
            let b = *text.add(i) as u8;
            *slot = b;
            if b == 0 {
                hit_nul = true;
            }
        }
    }
}

/// Rust port of `SetTextboxText` (0x004FB070, stdcall RET 0x20).
///
/// Writes `text` into the textbox's primary bitmap, optionally framing
/// it with a 1-pixel border + corner stipples when `border_color != 0`.
/// `*out_w` / `*out_h` receive the rendered text's pixel size (clamped
/// to the textbox's `max_width` / `max_height`). The returned pointer is
/// the bitmap that downstream blitters should use — usually
/// `primary_bitmap`, or `bitgrid_copy` for `scale < 1.0`.
///
/// `font_index` and `style_index` (the textbox's own field) jointly
/// stride [`FONT_ID_TABLE`] to pick the runtime `font_id` consumed by
/// DisplayGfx slot 7.
///
/// `fill_color == 0` clears the bitmap; nonzero fills it with `fill_color`
/// before drawing text.
///
/// `border_color != 0` enables the word-wrap measure-and-render path
/// plus a 1-pixel rectangular border with cornerpoint highlights and
/// outer-corner blackouts.
pub unsafe fn set_text(
    this: *mut Textbox,
    text: *const c_char,
    font_index: i32,
    fill_color: u32,
    border_color: u32,
    out_w: *mut i32,
    out_h: *mut i32,
    scale: Fixed,
) -> *mut DisplayBitGrid {
    unsafe {
        if font_index == (*this).cached_font_index
            && (fill_color as i32) == (*this).cached_fill_color
            && (border_color as i32) == (*this).cached_border_color
            && scale == (*this).cached_scale
            && cache_text_matches(&(*this).cached_text, text)
        {
            *out_w = (*this).cached_width;
            *out_h = (*this).cached_height;
            return (*this).return_bitmap;
        }

        let primary = (*this).primary_bitmap;
        let max_w = (*this).max_width;
        let max_h = (*this).max_height;

        if fill_color == 0 {
            bitmap::clear(primary);
        } else {
            bitmap::fill(primary, max_w, max_h, fill_color as u8);
        }

        let display = (*this).display;
        let font_id = resolve_font_id(font_index, (*this).style_index);
        let bitmap_grid = primary as *const DisplayBitGrid;

        if border_color == 0 {
            // Plain path: single slot-7 call at `pen_x = 0`, `pen_y = 0`.
            draw_text_on_bitmap(display, font_id, bitmap_grid, 0, 0, text, out_w, out_h);
        } else {
            // Border path: first call draws as much as fits at
            // `pen_x = 4` (left margin for the border) and returns the
            // count of characters drawn. If that count points at an
            // unfinished string we walk back to the last space and
            // re-render the tail one line below.
            let mut measure_chars =
                draw_text_on_bitmap(display, font_id, bitmap_grid, 4, 0, text, out_w, out_h);

            // The render-tail pass only runs when the first call left text
            // unfinished. For single-line text that fully fits — the common
            // case for worm health labels and the like — we skip straight to
            // the border-draw step. Running the second call unconditionally
            // would re-render starting at `pen_y = first_line_height`,
            // doubling `*out_h` and pushing the bottom border outside the
            // bitmap (corrupting adjacent heap memory in the process).
            if measure_chars >= 0 && *text.add(measure_chars as usize) != 0 {
                // Trim trailing words back to the last space, subtracting
                // the per-glyph advance from `*out_w`.
                let mut working_w = *out_w;
                let mut idx = measure_chars;
                let mut ch = *text.add(idx as usize) as u8;
                while ch != b' ' {
                    idx -= 1;
                    if idx < 0 {
                        break;
                    }
                    // Slot 9 writes the per-glyph advance to `out_1` and the
                    // font's max-glyph width to `out_2`; the trim loop only
                    // needs the advance.
                    let mut advance: u32 = 0;
                    let mut _max_glyph_w: u32 = 0;
                    get_font_metric(
                        display,
                        font_id,
                        *text.add(idx as usize) as u32,
                        &mut advance,
                        &mut _max_glyph_w,
                    );
                    working_w -= advance as i32;
                    ch = *text.add(idx as usize) as u8;
                }
                if idx >= 0 {
                    measure_chars = idx + 1;
                    *out_w = working_w;
                }

                // Render the wrapped tail one line below the first line.
                let saved_h = *out_h;
                let saved_w = *out_w;
                draw_text_on_bitmap(
                    display,
                    font_id,
                    bitmap_grid,
                    4,
                    saved_h,
                    text.add(measure_chars as usize),
                    out_w,
                    out_h,
                );
                if *out_w < saved_w {
                    *out_w = saved_w;
                }
                *out_h += saved_h;
            }

            // 1-pixel border + corner stipples.
            *out_w += 6;
            let bc = border_color as u8;
            DisplayBitGrid::fill_hline_raw(primary, 0, *out_w, 0, bc);
            DisplayBitGrid::fill_hline_raw(primary, 0, *out_w, *out_h, bc);
            DisplayBitGrid::fill_vline_raw(primary, 0, 0, *out_h, bc);
            DisplayBitGrid::fill_vline_raw(primary, *out_w, 0, *out_h, bc);
            // Inner-corner highlights in border_color, outer corners in 0.
            DisplayBitGrid::put_pixel_clipped_raw(primary, 1, 1, bc);
            DisplayBitGrid::put_pixel_clipped_raw(primary, *out_w - 1, 1, bc);
            DisplayBitGrid::put_pixel_clipped_raw(primary, 1, *out_h - 1, bc);
            DisplayBitGrid::put_pixel_clipped_raw(primary, *out_w - 1, *out_h - 1, bc);
            DisplayBitGrid::put_pixel_clipped_raw(primary, 0, 0, 0);
            DisplayBitGrid::put_pixel_clipped_raw(primary, *out_w, 0, 0);
            DisplayBitGrid::put_pixel_clipped_raw(primary, 0, *out_h, 0);
            DisplayBitGrid::put_pixel_clipped_raw(primary, *out_w, *out_h, 0);
        }

        // Clamp to textbox max dims (with the +1 fudge WA applies to
        // both axes after the border / measure loop).
        *out_w += 1;
        *out_h += 1;
        if *out_w > max_w {
            *out_w = max_w;
        }
        if *out_h > max_h {
            *out_h = max_h;
        }

        // Scale-down dither path: triggers only when `scale < 1.0` AND
        // changed since the last call. Lazy-allocates `bitgrid_copy`,
        // copies the primary into it, then dithers in place.
        let result_bitmap = if scale < Fixed::ONE && scale != (*this).cached_scale {
            let alpha = scale.min(Fixed(0x3333));
            if (*this).bitgrid_copy.is_null() {
                let copy = wa_malloc_struct_zeroed::<DisplayBitGrid>();
                if !copy.is_null() {
                    BitGrid::init(copy as *mut BitGrid, 8, max_w as u32, max_h as u32);
                    (*copy).vtable = rb(BIT_GRID_DISPLAY_VTABLE) as *const BitGridDisplayVtable;
                }
                (*this).bitgrid_copy = copy;
            }
            let copy = (*this).bitgrid_copy;
            bitmap::blit_sprite_rect_raw(0, 0, *out_w, *out_h, primary, 0, 0, 0, 0, copy);
            bitmap::dither(copy, *out_w, *out_h, (*this).lcg_state, alpha);
            (*this).lcg_state = wa_lcg((*this).lcg_state);
            copy
        } else {
            primary
        };

        (*this).return_bitmap = result_bitmap;
        (*this).cached_width = *out_w;
        (*this).cached_height = *out_h;
        (*this).cached_font_index = font_index;
        (*this).cached_scale = scale;
        (*this).cached_fill_color = fill_color as i32;
        (*this).cached_border_color = border_color as i32;
        cache_text_copy(&mut (*this).cached_text, text);

        result_bitmap
    }
}
