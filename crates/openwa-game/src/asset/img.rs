//! IMG format decoder — `.img` image files in `.dir` archives.
//!
//! The format parser is in [`openwa_core::img`]; the WA-facing wrappers
//! here adapt the stream/cached input formats to byte slices, build the
//! WA-side `BitGrid`, and update the global palette byte counter.

use crate::asset::gfx_dir::GfxDirStream;
use crate::bitgrid::{
    BIT_GRID_COLLISION_VTABLE, BIT_GRID_DISPLAY_VTABLE, BitGrid, CollisionBitGrid, DisplayBitGrid,
};
use crate::rebase::rb;
use crate::render::palette::{PaletteContext, palette_map_color};
use crate::wa_alloc::{wa_malloc_struct_zeroed, wa_malloc_zeroed};
use openwa_core::img::{
    IMG_MAGIC, img_decode as core_img_decode, img_decode_headerless as core_img_decode_headerless,
};

crate::define_addresses! {
    global G_SPRITE_PALETTE_BYTES = 0x007A_0870;
}

pub enum DecodedBitGrid {
    Display(*mut DisplayBitGrid),
    Collision(*mut CollisionBitGrid),
}

impl DecodedBitGrid {
    /// Get the underlying pointer as `*mut BitGrid` (common base type).
    /// Valid because all BitGrid variants share the same `#[repr(C)]` layout.
    pub fn as_bitgrid_ptr(self) -> *mut BitGrid {
        match self {
            DecodedBitGrid::Display(p) => p as *mut BitGrid,
            DecodedBitGrid::Collision(p) => p as *mut BitGrid,
        }
    }
}

/// Pure Rust implementation of IMG_Decode (0x4F5F80).
///
/// Reads an IMG image from a `GfxDirStream`, decodes it via
/// [`openwa_core::img::img_decode`], and produces a `BitGrid`
/// (DisplayBitGrid for 8bpp or CollisionBitGrid for 1bpp).
///
/// Convention: stdcall(palette_ctx, stream, align_flag), RET 0xC.
///
/// # Safety
/// All pointers must be valid WA objects. Must be called from within WA.exe.
#[cfg(target_arch = "x86")]
pub unsafe fn img_decode(
    palette_ctx: *mut PaletteContext,
    stream: *mut GfxDirStream,
    align_flag: i32,
) -> Option<DecodedBitGrid> {
    unsafe {
        // Read magic first — mismatch must short-circuit without further
        // stream reads, matching the original behavior.
        let mut magic: u32 = 0;
        GfxDirStream::read_raw(stream, &raw mut magic as *mut u8, 4);
        if magic != IMG_MAGIC {
            return None;
        }

        let mut data_size: u32 = 0;
        GfxDirStream::read_raw(stream, &raw mut data_size as *mut u8, 4);

        // Slurp the remaining IMG payload into a Vec, prefixed with the
        // magic + data_size we already read, so the core decoder sees the
        // whole file.
        if (data_size as usize) < 8 {
            return None;
        }
        let mut buf = vec![0u8; data_size as usize];
        buf[0..4].copy_from_slice(&magic.to_le_bytes());
        buf[4..8].copy_from_slice(&data_size.to_le_bytes());
        let remaining = data_size - 8;
        if remaining != 0 {
            GfxDirStream::read_raw(stream, buf.as_mut_ptr().add(8), remaining);
        }

        let decoded = core_img_decode(&buf, align_flag != 0, |rgb| {
            palette_map_color(palette_ctx, rgb) as u8
        })
        .ok()?;

        // Allocate BitGrid — 0x4C bytes matches the original allocation size
        // (base struct is 0x2C; the extra 0x20 is used by display variants).
        let grid = wa_malloc_zeroed(0x4C) as *mut BitGrid;
        if grid.is_null() {
            return None;
        }
        BitGrid::init(grid, decoded.bpp as u32, decoded.width, decoded.height);

        // Override vtable based on bpp
        if decoded.bpp == 8 {
            (*(grid as *mut DisplayBitGrid)).vtable =
                rb(BIT_GRID_DISPLAY_VTABLE) as *const crate::bitgrid::BitGridDisplayVtable;
        } else {
            (*(grid as *mut CollisionBitGrid)).vtable =
                rb(BIT_GRID_COLLISION_VTABLE) as *const crate::bitgrid::BitGridCollisionVtable;
        }

        // Core's row_stride matches BitGrid::init's formula; copy the
        // decoded pixels into the BitGrid's data buffer.
        debug_assert_eq!((*grid).row_stride, decoded.row_stride);
        core::ptr::copy_nonoverlapping(decoded.pixels.as_ptr(), (*grid).data, decoded.pixels.len());

        // Update global palette byte counter (mirrors the original's
        // side effect).
        if decoded.palette_rgb_bytes != 0 {
            let counter = rb(G_SPRITE_PALETTE_BYTES) as *mut u32;
            *counter = (*counter).wrapping_add(decoded.palette_rgb_bytes);
        }

        if decoded.bpp == 8 {
            Some(DecodedBitGrid::Display(grid as *mut DisplayBitGrid))
        } else {
            Some(DecodedBitGrid::Collision(grid as *mut CollisionBitGrid))
        }
    }
}

/// Pure Rust implementation of DisplayGfx__Constructor (0x4F5E80).
///
/// Reads from a raw memory buffer (cached `.dir` entry). Always creates an
/// 8bpp DisplayBitGrid with `external_buffer = 1` (pixel data lives in the
/// cached buffer). Format parsing happens in [`openwa_core::img::img_decode_headerless`];
/// we then write the remapped pixels back into the cache buffer at the
/// aligned offset and hand the `BitGrid` a pointer into that buffer.
///
/// In the original WA code, the PaletteContext is passed implicitly via EBX
/// (callee-saved register from the caller). Our Rust API takes it explicitly.
///
/// # Safety
/// `raw_image` must be a valid pointer to cached image data with at least
/// the bytes the header describes (10-byte prefix + palette + width/height
/// + aligned pixel block).
/// `palette_ctx` must be a valid PaletteContext pointer (from the caller).
#[cfg(target_arch = "x86")]
pub unsafe fn img_decode_cached(
    palette_ctx: *mut PaletteContext,
    raw_image: *mut u8,
) -> *mut DisplayBitGrid {
    unsafe {
        // Peek at the header to determine the total byte span we need to
        // expose to the safe decoder. The format is self-describing up to
        // the pixel block, and the pixel block is width*height bytes.
        let palette_count = *(raw_image.add(0x0A) as *const u16) as usize;
        let wh_start = 0x0C + palette_count * 3;
        let width = *(raw_image.add(wh_start) as *const u16) as u32;
        let height = *(raw_image.add(wh_start + 2) as *const u16) as u32;
        let aligned_offset = (wh_start + 7) & !3usize;
        let total = aligned_offset + (width as usize) * (height as usize);

        let raw = core::slice::from_raw_parts(raw_image, total);
        let decoded = match core_img_decode_headerless(raw, |rgb| {
            palette_map_color(palette_ctx, rgb) as u8
        }) {
            Ok(d) => d,
            Err(_) => return core::ptr::null_mut(),
        };

        // Preserve external-buffer semantics: the BitGrid's `data` points
        // into the cached buffer. Write the remapped pixels back in place.
        let pixel_dst = raw_image.add(aligned_offset);
        core::ptr::copy_nonoverlapping(decoded.pixels.as_ptr(), pixel_dst, decoded.pixels.len());

        let grid = wa_malloc_struct_zeroed::<DisplayBitGrid>();
        if grid.is_null() {
            return core::ptr::null_mut();
        }

        (*grid).vtable = rb(BIT_GRID_DISPLAY_VTABLE) as *const crate::bitgrid::BitGridDisplayVtable;
        (*grid).cells_per_unit = 8;
        (*grid).width = decoded.width;
        (*grid).height = decoded.height;
        (*grid).row_stride = decoded.row_stride; // = width for cached 8bpp
        (*grid).data = pixel_dst;
        (*grid).clip_left = 0;
        (*grid).clip_top = 0;
        (*grid).clip_right = decoded.width;
        (*grid).clip_bottom = decoded.height;
        (*grid).external_buffer = 1;

        if decoded.palette_rgb_bytes != 0 {
            let counter = rb(G_SPRITE_PALETTE_BYTES) as *mut u32;
            *counter = (*counter).wrapping_add(decoded.palette_rgb_bytes);
        }

        grid
    }
}
