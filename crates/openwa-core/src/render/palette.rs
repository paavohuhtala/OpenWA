//! Palette context — maps RGB colors to display palette indices.
//!
//! WA's `PaletteContext` maintains a 256-entry palette with a free-slot
//! allocator and a recently-mapped cache for fast repeated lookups.

/// PaletteContext struct layout.
///
/// Used by `PaletteContext__MapColor` (0x5412B0) and related functions.
/// May be embedded in a larger composite object (e.g. GfxDir + palette data).
#[repr(C)]
pub struct PaletteContext {
    /// +0x00: Lowest palette index in the current update batch.
    /// Set by the caller before `update_palette`; used to expand `palette_dirty_min`.
    pub dirty_range_min: i16,
    /// +0x02: Highest palette index in the current update batch.
    pub dirty_range_max: i16,
    /// +0x04: RGB values stored per palette index (24-bit RGB in low 3 bytes).
    pub rgb_table: [u32; 256],
    /// +0x404: In-use flag per palette index (1 = allocated).
    pub in_use: [u8; 256],
    /// +0x504: Number of free slots remaining in the free stack.
    pub free_count: i16,
    /// +0x506: Free slot index stack (LIFO). Entries are palette indices.
    pub free_stack: [u8; 256],
    /// +0x606: Number of entries in the recently-mapped cache.
    pub cache_count: i16,
    /// +0x608: Recently-mapped palette indices (cache for fast lookup).
    pub cache: [u8; 256],
    /// +0x708: Dirty flag (set to 1 when a new slot is allocated).
    pub dirty: u16,
    /// +0x70A: Iteration position in cache[] during `update_palette`.
    /// Read/written by `update_palette` to track progress through the cache.
    pub cache_iter: i16,
}

const _: () = assert!(core::mem::offset_of!(PaletteContext, dirty_range_min) == 0x00);
const _: () = assert!(core::mem::offset_of!(PaletteContext, dirty_range_max) == 0x02);
const _: () = assert!(core::mem::offset_of!(PaletteContext, rgb_table) == 0x04);
const _: () = assert!(core::mem::offset_of!(PaletteContext, in_use) == 0x404);
const _: () = assert!(core::mem::offset_of!(PaletteContext, free_count) == 0x504);
const _: () = assert!(core::mem::offset_of!(PaletteContext, free_stack) == 0x506);
const _: () = assert!(core::mem::offset_of!(PaletteContext, cache_count) == 0x606);
const _: () = assert!(core::mem::offset_of!(PaletteContext, cache) == 0x608);
const _: () = assert!(core::mem::offset_of!(PaletteContext, dirty) == 0x708);
const _: () = assert!(core::mem::offset_of!(PaletteContext, cache_iter) == 0x70A);

/// Map an RGB color to the nearest display palette index.
///
/// Rust port of `PaletteContext__MapColor` (0x5412B0). Operates on a raw
/// pointer to WA's PaletteContext struct.
///
/// Algorithm:
/// 1. Search the recently-mapped cache for an exact 24-bit RGB match
/// 2. On miss: pop a free slot, store the RGB, add to cache, return new index
/// 3. If no free slots: return 0xFFFFFFFF
///
/// # Safety
///
/// `ctx` must point to a valid WA PaletteContext struct.
pub unsafe fn palette_map_color(ctx: *mut PaletteContext, rgb: u32) -> u32 {
    // Raw pointer ops throughout — no &mut to avoid noalias miscompilation,
    // and to match WA's overflow behavior when cache_count reaches 256
    // (writes past cache[] into dirty field, which is then set to 1 anyway).
    let p = ctx as *mut u8;

    let cache_count = *(p.add(0x606) as *const i16);

    // Search cache for matching RGB (24-bit compare).
    // Clamp to 256 — WA doesn't bounds-check cache_count.
    let search_len = (cache_count as usize).min(256);
    for i in 0..search_len {
        let idx = *p.add(0x608 + i) as usize;
        if idx != 0 && (*(p.add(0x04 + idx * 4) as *const u32) ^ rgb) & 0xFF_FFFF == 0 {
            return idx as u32;
        }
    }

    // Cache miss — allocate a free slot
    let free_count = *(p.add(0x504) as *const i16);
    if free_count <= 0 {
        return 0xFFFF_FFFF;
    }

    let new_free_count = free_count - 1;
    *(p.add(0x504) as *mut i16) = new_free_count;
    let slot = *p.add(0x506 + new_free_count as usize);
    let slot_idx = slot as usize;

    // Add to cache. WA doesn't bounds-check: when cache_count == 256 it writes
    // past cache[] into the dirty field, which is set to 1 immediately after.
    // We just skip the write instead.
    if (cache_count as usize) < 256 {
        *p.add(0x608 + cache_count as usize) = slot;
    }
    *(p.add(0x606) as *mut i16) = cache_count + 1;

    // Store RGB and mark in-use
    *(p.add(0x04 + slot_idx * 4) as *mut u32) = rgb;
    *p.add(0x404 + slot_idx) = 1;
    *(p.add(0x708) as *mut u16) = 1;

    slot_idx as u32
}

/// Remap each pixel in a buffer through a 256-byte lookup table.
///
/// Port of FUN_005b2beb (stdcall, RET 0x14). Used by `load_sprite_by_name`
/// to apply the palette mapping to freshly-loaded sprite pixel data.
///
/// Processes `width_dwords * 4` bytes per row, advancing by `pitch` bytes
/// between rows.
///
/// # Safety
/// - `data` must point to a pixel buffer with at least `height` rows of
///   `pitch` bytes each.
/// - `lut` must point to a 256-byte lookup table.
/// - `width_dwords * 4` must not exceed `pitch`.
pub unsafe fn remap_pixels_through_lut(
    data: *mut u8,
    pitch: u32,
    lut: *const u8,
    width_dwords: u32,
    height: u32,
) {
    let pixel_count = width_dwords * 4;
    let mut row_ptr = data;
    for _ in 0..height {
        let mut p = row_ptr;
        for _ in 0..pixel_count {
            *p = *lut.add(*p as usize);
            p = p.add(1);
        }
        row_ptr = row_ptr.add(pitch as usize);
    }
}

crate::define_addresses! {
    class "PaletteContext" {
        /// PaletteContext__Init — usercall EAX=ctx* (no stack params)
        fn/Usercall PALETTE_CONTEXT_INIT = 0x0054_11A0;
        /// PaletteContext__InitRange — usercall ESI=ctx*, 2 stack params (range_min, range_max)
        fn/Usercall PALETTE_CONTEXT_INIT_RANGE = 0x0054_1170;
        /// PaletteContext__MapColor — thiscall(palette_ctx, rgb_u32), returns nearest palette index
        fn/Thiscall PALETTE_CONTEXT_MAP_COLOR = 0x0054_12B0;
    }
}
