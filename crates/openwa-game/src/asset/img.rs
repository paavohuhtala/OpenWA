//! IMG format decoder — `.img` image files in `.dir` archives.
//!
//! WA uses a custom "IMG" format for static graphics (gradients, masks,
//! HUD elements). Format: magic "IMG\x1A", header with flags, optional
//! palette, width/height, then raw or LZSS-compressed pixel data.
//!
//! Two entry points:
//! - `img_decode` — reads from a `GfxDirStream` (stream-based path)
//! - `img_decode_cached` — reads from a raw memory buffer (cached path)

use crate::asset::gfx_dir::GfxDirStream;
use crate::bitgrid::{
    BIT_GRID_COLLISION_VTABLE, BIT_GRID_DISPLAY_VTABLE, BitGrid, CollisionBitGrid, DisplayBitGrid,
};
use crate::rebase::rb;
use crate::render::palette::{PaletteContext, palette_map_color, remap_pixels_through_lut};
use crate::render::sprite::lzss::sprite_lzss_decode;
use crate::wa_alloc::wa_malloc_zeroed;

/// IMG file magic: "IMG\x1A" as little-endian u32.
const IMG_MAGIC: u32 = 0x1A47_4D49;

/// IMG header flags.
const FLAG_HAS_PALETTE: u16 = 0x8000;
const FLAG_LZSS_COMPRESSED: u16 = 0x4000;
const FLAG_EXTENDED_HEADER: u16 = 0x3FF6;

// ─── Global sprite palette byte counter (0x7A0870) ──────────────────────────
// IMG_Decode and DisplayGfx__Constructor both add palette_count * 3 to this.
crate::define_addresses! {
    global G_SPRITE_PALETTE_BYTES = 0x007A_0870;
    /// Global temp buffer for LZSS decompression (large static array in WA.exe)
    global G_LZSS_TEMP_BUFFER = 0x006B_39C8;
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
/// Reads an IMG image from a `GfxDirStream`, decodes it into a BitGrid
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
        // ── 1. Read and validate magic ──
        let mut magic: u32 = 0;
        GfxDirStream::read_raw(stream, &raw mut magic as *mut u8, 4);
        if magic != IMG_MAGIC {
            return None;
        }

        // ── 2. Read data_size (used for LZSS remaining calculation) ──
        let mut data_size: u32 = 0;
        GfxDirStream::read_raw(stream, &raw mut data_size as *mut u8, 4);

        // ── 3. Read flags ──
        let mut flags: u16 = 0;
        GfxDirStream::read_raw(stream, &raw mut flags as *mut u8, 2);

        // ── 4. Handle extended header (rare) ──
        if (flags & FLAG_EXTENDED_HEADER) != 0 {
            // Seek back before flags, skip bytes until null terminator, re-read flags
            let pos = GfxDirStream::bytes_consumed_raw(stream);
            GfxDirStream::seek_raw(stream, pos.wrapping_sub(2));
            loop {
                let mut b: u8 = 0;
                GfxDirStream::read_raw(stream, &raw mut b, 1);
                if b == 0 {
                    break;
                }
            }
            GfxDirStream::read_raw(stream, &raw mut flags as *mut u8, 2);
        }

        // ── 5. Build palette LUT ──
        let mut palette_lut = [0u8; 256];
        if (flags & FLAG_HAS_PALETTE) != 0 {
            let mut palette_count: u16 = 0;
            GfxDirStream::read_raw(stream, &raw mut palette_count as *mut u8, 2);

            // Read RGB triplets into temp buffer
            let rgb_byte_count = palette_count as u32 * 3;
            let mut rgb_buf = [0u8; 256 * 3]; // max 256 entries * 3 bytes
            GfxDirStream::read_raw(stream, rgb_buf.as_mut_ptr(), rgb_byte_count);

            // Map each RGB triplet through PaletteContext to build LUT
            // lut[0] = 0 (transparent), lut[1+i] = MapColor(RGB[i])
            for i in 0..palette_count as usize {
                // Read as u32 from the RGB bytes (only low 3 bytes matter)
                let r = rgb_buf[i * 3] as u32;
                let g = rgb_buf[i * 3 + 1] as u32;
                let b = rgb_buf[i * 3 + 2] as u32;
                let rgb = r | (g << 8) | (b << 16);
                palette_lut[i + 1] = palette_map_color(palette_ctx, rgb) as u8;
            }

            // Update global palette byte counter
            let counter = rb(G_SPRITE_PALETTE_BYTES) as *mut u32;
            *counter = (*counter).wrapping_add(rgb_byte_count);
        }

        // ── 6. Read width and height ──
        let mut width: u16 = 0;
        let mut height: u16 = 0;
        GfxDirStream::read_raw(stream, &raw mut width as *mut u8, 2);
        GfxDirStream::read_raw(stream, &raw mut height as *mut u8, 2);

        // ── 7. Determine bpp from flags low byte ──
        let bpp = (flags & 0xFF) as u8;
        if bpp != 1 && bpp != 8 {
            return None;
        }

        // ── 8. Allocate BitGrid (0x4C bytes, matching original allocation size) ──
        let grid = wa_malloc_zeroed(0x4C) as *mut BitGrid;
        if grid.is_null() {
            return None;
        }
        BitGrid::init(grid, bpp as u32, width as u32, height as u32);

        // Override vtable based on bpp
        if bpp == 8 {
            (*(grid as *mut DisplayBitGrid)).vtable =
                rb(BIT_GRID_DISPLAY_VTABLE) as *const crate::bitgrid::BitGridDisplayVtable;
        } else {
            (*(grid as *mut CollisionBitGrid)).vtable =
                rb(BIT_GRID_COLLISION_VTABLE) as *const crate::bitgrid::BitGridCollisionVtable;
        }

        let pixel_data = (*grid).data;
        let row_stride = (*grid).row_stride;

        // ── 9. Alignment padding ──
        if align_flag != 0 {
            loop {
                let pos = GfxDirStream::bytes_consumed_raw(stream);
                if (pos & 3) == 0 {
                    break;
                }
                let mut dummy: u8 = 0;
                GfxDirStream::read_raw(stream, &raw mut dummy, 1);
            }
        }

        // ── 10. Read pixel data ──
        if (flags & FLAG_LZSS_COMPRESSED) != 0 {
            // LZSS path: read remaining stream data into temp buffer, then decompress.
            // sprite_lzss_decode applies the palette LUT during decompression,
            // so no separate remap step is needed afterward.
            let pos = GfxDirStream::bytes_consumed_raw(stream);
            let remaining = data_size.wrapping_sub(pos);

            // Use WA's global LZSS temp buffer
            let lzss_buffer = rb(G_LZSS_TEMP_BUFFER) as *mut u8;
            GfxDirStream::read_raw(stream, lzss_buffer, remaining);

            sprite_lzss_decode(pixel_data, lzss_buffer, palette_lut.as_ptr());
        } else {
            // Row-by-row path: read raw pixel data
            let bits_per_row = width as u32 * bpp as u32;
            let bytes_per_row = bits_per_row.div_ceil(8);

            for row in 0..height as u32 {
                let row_ptr = pixel_data.add((row * row_stride) as usize);
                GfxDirStream::read_raw(stream, row_ptr, bytes_per_row);
            }

            // Remap through palette LUT (only for non-LZSS, since LZSS applies the LUT inline)
            if (flags & FLAG_HAS_PALETTE) != 0 && bpp == 8 {
                let dwords_per_row = row_stride / 4;
                remap_pixels_through_lut(
                    pixel_data,
                    row_stride,
                    palette_lut.as_ptr(),
                    dwords_per_row,
                    height as u32,
                );
            }
        }

        if bpp == 8 {
            Some(DecodedBitGrid::Display(grid as *mut DisplayBitGrid))
        } else {
            Some(DecodedBitGrid::Collision(grid as *mut CollisionBitGrid))
        }
    }
}

/// Pure Rust implementation of DisplayGfx__Constructor (0x4F5E80).
///
/// Reads from a raw memory buffer (cached `.dir` entry). Always creates an
/// 8bpp DisplayBitGrid. Format: raw_image+0x0A has palette_count, RGB data,
/// width, height, then pixel data.
///
/// In the original WA code, the PaletteContext is passed implicitly via EBX
/// (callee-saved register from the caller). Our Rust API takes it explicitly.
///
/// # Safety
/// `raw_image` must be a valid pointer to cached image data.
/// `palette_ctx` must be a valid PaletteContext pointer (from the caller).
#[cfg(target_arch = "x86")]
pub unsafe fn img_decode_cached(
    palette_ctx: *mut PaletteContext,
    raw_image: *mut u8,
) -> *mut DisplayBitGrid {
    unsafe {
        use crate::wa_alloc::wa_malloc_struct_zeroed;

        let mut ptr = raw_image.add(0x0A);

        // ── 1. Read palette count and build LUT ──
        let palette_count = *(ptr as *const u16) as usize;
        ptr = ptr.add(2);

        let mut palette_lut = [0u8; 256];
        // palette_lut[0] = 0 (transparent, already zero)

        for i in 0..palette_count {
            // Read 3 RGB bytes as a u32 (4th byte is junk but MapColor only uses low 24 bits)
            let rgb = *(ptr as *const u32);
            palette_lut[i + 1] = palette_map_color(palette_ctx, rgb) as u8;
            ptr = ptr.add(3);
        }

        // Update global palette byte counter
        let counter = rb(G_SPRITE_PALETTE_BYTES) as *mut u32;
        *counter = (*counter).wrapping_add(palette_count as u32 * 3);

        // ── 2. Read width and height ──
        let width = *(ptr as *const u16) as u32;
        let height = *(ptr.add(2) as *const u16) as u32;
        ptr = ptr.add(2); // advance past width (height pointer computed differently)

        // ── 3. Allocate DisplayBitGrid (0x4C bytes, 8bpp only) ──
        let grid = wa_malloc_struct_zeroed::<DisplayBitGrid>();
        if grid.is_null() {
            return core::ptr::null_mut();
        }

        // ── 4. Compute pixel data pointer (4-byte aligned after width+height) ──
        // Original: (ptr - raw_image + 5) & ~3 + raw_image
        // ptr is at raw_image + 0x0C + palette_count * 3 + 2
        let offset = ptr.offset_from(raw_image) as usize + 5; // +5 accounts for remaining header
        let aligned_offset = (offset & !3) as usize;
        let pixel_data = raw_image.add(aligned_offset);

        // ── 5. Set up fields manually (matching original, not via BitGrid::init) ──
        (*grid).vtable = rb(BIT_GRID_DISPLAY_VTABLE) as *const crate::bitgrid::BitGridDisplayVtable;
        (*grid).cells_per_unit = 8;
        (*grid).width = width;
        (*grid).height = height;
        (*grid).row_stride = width; // row_stride = width for 8bpp uncompressed
        (*grid).data = pixel_data;
        (*grid).clip_left = 0;
        (*grid).clip_top = 0;
        (*grid).clip_right = width;
        (*grid).clip_bottom = height;
        (*grid).external_buffer = 1; // pixel data is owned by the cached buffer, not the grid

        // ── 6. Remap pixels through palette LUT ──
        if (*grid).cells_per_unit == 8 {
            let dwords_per_row = width / 4;
            remap_pixels_through_lut(
                pixel_data,
                width,
                palette_lut.as_ptr(),
                dwords_per_row,
                height,
            );
        }

        grid
    }
}
