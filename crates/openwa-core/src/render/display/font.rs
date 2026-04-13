// =========================================================================
// Font loading
// =========================================================================

use std::ffi::c_char;

use crate::asset::gfx_dir::GfxDir;
use crate::render::palette::PaletteContext;

/// Font object — 0x1C bytes.
///
/// Allocated by `load_font` (vtable slot 34) from a `.fnt` file loaded via
/// GfxDir. Referenced from `DisplayBase::font_table` (offset 0x309C). Read by
/// `Font__GetInfo` / `Font__GetMetric` / `Font__DrawText` / `Font__SetPalette`
/// to render bitmap text.
///
/// Field semantics derived from `FUN_004f99d0` (the parser) and `Font__GetInfo`
/// (0x4fa7d0, which confirms width at +0, width_div_5 at +2, glyph count at +4,
/// and glyph table at +8 with 12-byte entries whose byte +4 is a max metric).
#[repr(C)]
pub struct Font {
    /// 0x00: Font max width in pixels (read from .fnt metadata).
    pub width: u16,
    /// 0x02: `width / 5 + 1` — initial max metric seed in `Font__GetInfo`.
    pub width_div_5: u16,
    /// 0x04: Glyph count / font height.
    pub height: u16,
    /// 0x06: Duplicate of height.
    pub _height2: u16,
    /// 0x08: Pointer to glyph table (height entries of 12 bytes each).
    pub glyph_table: *mut GlyphEntry,
    /// 0x0C: Pointer to a 256-byte char→glyph-index lookup table that lives
    /// in the .fnt buffer immediately after the RGB palette triplets.
    /// Each entry is a 1-based glyph index (or 0 if the codepoint has no
    /// glyph) — `Font__GetMetric` / `Font__SetParam` use it to map a string
    /// byte to its `glyph_table[index - 1]` entry. `font_extend` writes new
    /// codepoints into this table.
    pub char_to_glyph_idx: *mut u8,
    /// 0x10: Pointer to remapped pixel data in the source buffer.
    pub pixel_data: *mut u8,
    /// 0x14: Zeroed by `load_font`; purpose unknown.
    pub _unknown_14: u32,
    /// 0x18: When non-null, holds the address of an auxiliary allocation
    /// owned by this font (a separate buffer of glyph entries + pixel data
    /// pointed at by individual `GlyphEntry::pixel_offset` deltas). Zeroed
    /// by `load_font`; written by `font_extend` (which appends new glyphs
    /// from a `.fex` file) and `font_set_palette_impl` (which derives the
    /// `.`/`;` glyphs for the digital font). The cleanup path frees this
    /// when the FontObject is destroyed.
    pub aux_alloc: u32,
}

const _: () = assert!(core::mem::size_of::<Font>() == 0x1C);

/// Per-glyph metadata in a `.fnt` file (12 bytes).
///
/// Indexed by `FontObject::glyph_table`. Layout matches the loop stride of 0xC
/// used by the parser, `Font__GetInfo` (0x4fa7d0), and `Font__GetMetric` (0x4fa780).
///
/// `Font__GetMetric` reads `width` (offset +4) as the character advance metric.
/// `Font__DrawText` uses `pixel_offset` as a byte delta from
/// `FontObject::pixel_data` to locate the glyph's bitmap rows. The offset can
/// be negative when `font_extend` adds glyphs from a separate allocation.
#[repr(C)]
pub struct GlyphEntry {
    /// 0x00: Glyph bounding-box top-left X within the glyph cell.
    pub start_x: u16,
    /// 0x02: Glyph bounding-box top-left Y within the glyph cell.
    pub start_y: u16,
    /// 0x04: Glyph bounding-box width in pixels (also used as the advance metric).
    pub width: u16,
    /// 0x06: Glyph bounding-box height in pixels.
    pub height: u16,
    /// 0x08: Byte delta from `FontObject::pixel_data` to this glyph's bitmap.
    /// Treated as a signed offset by drawing code so `font_extend` can point
    /// glyphs at pixels that live in a different allocation.
    pub pixel_offset: u32,
}

const _: () = assert!(core::mem::size_of::<GlyphEntry>() == 0xC);

/// Fixed prefix of a `.fnt` binary blob (0xC bytes).
///
/// Followed in memory by: packed RGB triplets (`palette_count * 3` bytes),
/// a 0x100-byte character-width table, `max_width: i16`, `glyph_count: u16`,
/// 4-byte alignment padding, `GlyphEntry[glyph_count]`, then the packed
/// glyph pixel data. `font_parse_data` walks this variable-length tail using
/// byte pointers rather than a single `#[repr(C)]` struct.
#[repr(C)]
pub struct FntHeader {
    /// 0x00: Unknown / version.
    pub _unknown_00: u32,
    /// 0x04: Total size of this record in bytes, including this field.
    pub data_size: i32,
    /// 0x08: Unknown u16 (skipped by the parser).
    pub _unknown_08: u16,
    /// 0x0A: Number of palette entries that follow this header.
    pub palette_count: u16,
    // +0x0C: packed RGB triplets (variable length)
}

const _: () = assert!(core::mem::size_of::<FntHeader>() == 0xC);

/// Port of `FUN_004f99d0` — parses `.fnt` binary data into a `FontObject`.
///
/// Original convention: cdecl with 1 stack arg (buffer), `unaff_EDI = font_obj`.
/// Ported as a plain Rust function.
///
/// Buffer layout:
/// ```text
/// +0x00  u32        (unknown / version)
/// +0x04  i32        data_size (total bytes in this record, including this field)
/// +0x08  u16        (unknown)
/// +0x0A  u16        palette_count
/// +0x0C  RGB[palette_count * 3]  packed RGB triplets
/// +...   u8[0x100]  character-width table (1 byte per codepoint)
/// +0x100 u16        max_width
/// +0x102 u16        glyph_count / height
/// +...               align to 4 from buffer start
/// +...   GlyphEntry[glyph_count]  12 bytes each (start_x/start_y/end_x/end_y/...)
/// +...   u8[]       packed 4bpp glyph pixels (remapped in place via palette LUT)
/// ```
///
/// # Safety
/// `font_obj` must be a valid writable `FontObject`. `header` must point to a
/// freshly-loaded `.fnt` blob at least `header.data_size` bytes long and
/// followed in memory by the variable-length tail described above.
/// `palette_ctx` must be a valid `PaletteContext`.
pub unsafe fn font_parse_data(
    font_obj: *mut Font,
    header: *mut FntHeader,
    palette_ctx: *mut PaletteContext,
) -> u32 {
    use crate::render::palette::{palette_map_color, remap_pixels_through_lut};

    let data_size = (*header).data_size;
    let palette_count = (*header).palette_count as i32;
    let base_bytes = header as *mut u8;

    // Cursor starts at the first byte after the fixed header (the RGB triplets).
    let mut cursor = base_bytes.add(core::mem::size_of::<FntHeader>());

    // Build 256-byte palette LUT. Entry 0 = map_color(0); entries 1..=palette_count
    // come from the RGB triplets. The original reads 4 bytes per triplet but
    // advances by 3 — only the low 24 bits matter.
    let mut palette_lut = [0u8; 256];
    palette_lut[0] = palette_map_color(palette_ctx, 0) as u8;
    for i in 0..palette_count {
        let rgb = *(cursor as *const u32);
        palette_lut[(1 + i) as usize] = palette_map_color(palette_ctx, rgb) as u8;
        cursor = cursor.add(3);
    }

    // char_to_glyph_idx points to the 256-byte char→glyph-index table that
    // sits immediately after the RGB palette triplets in the .fnt buffer.
    (*font_obj).char_to_glyph_idx = cursor;

    // Read max_width (i16) at cursor + 0x100 — the character-width table sits
    // between the palette and the header.
    let max_width = *(cursor.add(0x100) as *const i16) as i32;
    cursor = cursor.add(0x100);
    (*font_obj).width = max_width as u16;

    // Seed value for Font__GetInfo's max metric: signed (width / 5) + 1,
    // with a +1 correction when the quotient is negative (matches the
    // IMUL+SAR+SHR+LEA sequence in the original).
    let w_div_5 = max_width / 5;
    let seed = w_div_5.wrapping_add((w_div_5 >> 31) & 1).wrapping_add(1);
    (*font_obj).width_div_5 = seed as u16;

    // Skip max_width, read glyph_count (u16).
    cursor = cursor.add(2);
    let glyph_count = *(cursor as *const u16);
    cursor = cursor.add(2);
    (*font_obj).height = glyph_count;
    (*font_obj)._height2 = glyph_count;

    // Align cursor to 4 bytes relative to the buffer start.
    let base_addr = base_bytes as usize;
    while (cursor as usize - base_addr) & 3 != 0 {
        cursor = cursor.add(1);
    }

    // Glyph table: glyph_count entries of 12 bytes each.
    let glyph_table = cursor as *mut GlyphEntry;
    (*font_obj).glyph_table = glyph_table;
    let pixel_data = glyph_table.add(glyph_count as usize) as *mut u8;
    (*font_obj).pixel_data = pixel_data;

    // Pixel byte count: data_size - (pixel_data - buffer), rounded down to dwords.
    // Original uses CDQ+AND 3+ADD+SAR 2 which rounds toward zero.
    let remaining = data_size - (pixel_data as usize - base_addr) as i32;
    let dword_count = remaining
        .wrapping_add((remaining >> 31) & 3)
        .wrapping_shr(2) as u32;

    remap_pixels_through_lut(pixel_data, 0, palette_lut.as_ptr(), dword_count, 1);

    1
}

/// Port of `FUN_004f9940` — loads a font resource by name from a GfxDir and
/// feeds it into `font_parse_data`.
///
/// Original convention: usercall with `EAX = gfx_dir`, `ECX = font_obj`,
/// `stack[0] = layer_ctx`, `stack[1] = filename`, `RET 0x8`. The function
/// first tries `GfxDir__FindEntry` + cached-load via `gfx_dir->vtable[2]`;
/// on miss it falls back to `GfxDir__LoadImage` + `image->vtable[4]` (get_size)
/// + `image->vtable[5]` (read into caller-allocated buffer) + `image->vtable[0](1)`
/// (destroy), then tail-calls the parser.
///
/// Note: `layer_ctx` is only used by `font_parse_data` (as the PaletteContext).
/// The original never touches it in this function — it just forwards it via
/// ECX to the tail call.
///
/// # Safety
/// `font_obj` must be a valid writable `FontObject`. `gfx_dir` must be a valid
/// GfxDir. `palette_ctx` must be a valid PaletteContext. `name` must be a
/// valid null-terminated C string.
pub unsafe fn font_load_from_gfx(
    font_obj: *mut Font,
    gfx_dir: *mut GfxDir,
    palette_ctx: *mut PaletteContext,
    name: *const c_char,
) -> u32 {
    use crate::asset::gfx_dir::{
        gfx_dir_find_entry, gfx_dir_load_image, GfxDirEntry, GfxDirStream,
    };
    use crate::wa_alloc::wa_malloc;

    // 1. Try FindEntry → gfx_dir->vtable[2](entry->value) for cached load.
    let entry = gfx_dir_find_entry(name, gfx_dir);
    if !entry.is_null() {
        let entry_val = (*(entry as *const GfxDirEntry)).value;
        let cached = GfxDir::load_cached_raw(gfx_dir, entry_val) as *mut FntHeader;
        if !cached.is_null() {
            return font_parse_data(font_obj, cached, palette_ctx);
        }
    }

    // 2. Fallback: LoadImage → get_total_size → wa_malloc → read → destroy
    let image = gfx_dir_load_image(gfx_dir, name);
    if image.is_null() {
        return 0;
    }

    let size = GfxDirStream::get_total_size_raw(image);

    // Match the original's allocation: round size up to 4-byte multiple, add 0x20 guard.
    let alloc_size = ((size + 3) & !3u32).wrapping_add(0x20);
    let buffer = wa_malloc(alloc_size);
    // Original memsets only `size` bytes, not the full allocation.
    if !buffer.is_null() {
        core::ptr::write_bytes(buffer, 0, size as usize);
    }

    GfxDirStream::read_raw(image, buffer, size);
    GfxDirStream::destroy_raw(image);

    font_parse_data(font_obj, buffer as *mut FntHeader, palette_ctx)
}

/// Read a glyph's advance metric from the glyph table. Returns
/// `glyph_table[idx_1based - 1].width as i32 + 1` (matching the original's
/// `*(ushort*)(glyph_table - 8 + idx*0xC) + 1`).
#[inline]
pub(crate) unsafe fn glyph_advance_metric(font_obj: *const Font, idx_1based: u8) -> i32 {
    let entry = (*font_obj).glyph_table.add(idx_1based as usize - 1);
    (*entry).width as i32 + 1
}

/// Pure-Rust port of `Font__GetInfo` (0x4FA7D0).
///
/// Writes the font's max width to `*out_width` and the longest glyph
/// advance metric (max of `width_div_5` and `glyph.width + 1` for every
/// glyph) to `*out_max_metric`. Returns 1 unconditionally.
///
/// # Safety
/// `font_obj` must be a valid `*const FontObject` with a glyph table of at
/// least `font_obj.height` entries. The output pointers must be writable.
pub unsafe fn font_get_info_impl(
    font_obj: *const Font,
    out_max_metric: *mut i32,
    out_width: *mut i32,
) -> u32 {
    *out_width = (*font_obj).width as i16 as i32;
    let mut max_metric = (*font_obj).width_div_5 as i16 as i32;
    let height = (*font_obj).height as i32;
    for i in 0..height {
        let entry = (*font_obj).glyph_table.add(i as usize);
        let candidate = (*entry).width as i32 + 1;
        if max_metric < candidate {
            max_metric = candidate;
        }
    }
    *out_max_metric = max_metric;
    1
}

/// Pure-Rust port of `Font__GetMetric` (0x4FA780).
///
/// Always writes the font's max width to `*out_width`. For space (`0x20`)
/// and non-breaking space (`0xA0`), writes `width_div_5` to `*out_metric`
/// and returns 1. Otherwise looks up the codepoint in `char_to_glyph_idx`;
/// if the codepoint is unmapped (entry is 0) returns 0, else writes
/// `glyph_table[idx-1].width + 1` to `*out_metric` and returns 1.
///
/// # Safety
/// `font_obj` must be a valid `*const FontObject`. The output pointers must
/// be writable.
pub unsafe fn font_get_metric_impl(
    font_obj: *const Font,
    char_code: u8,
    out_metric: *mut i32,
    out_width: *mut i32,
) -> u32 {
    *out_width = (*font_obj).width as i16 as i32;
    if char_code != 0x20 && char_code != 0xA0 {
        let glyph_idx = *(*font_obj).char_to_glyph_idx.add(char_code as usize);
        if glyph_idx == 0 {
            return 0;
        }
        *out_metric = glyph_advance_metric(font_obj, glyph_idx);
        return 1;
    }
    *out_metric = (*font_obj).width_div_5 as i16 as i32;
    1
}

/// Pure-Rust port of `Font__SetParam` (0x4FA720).
///
/// Walks the null-terminated byte string `text`, accumulating each
/// codepoint's advance metric (`width_div_5` for unmapped codepoints,
/// `glyph.width + 1` for mapped ones) into `*out_total`. Always writes the
/// font's max width to `*out_width`.
///
/// # Safety
/// `font_obj` must be a valid `*const FontObject`. `text` must be a valid
/// null-terminated byte string. Output pointers must be writable.
pub unsafe fn font_set_param_impl(
    font_obj: *const Font,
    text: *const u8,
    out_total: *mut i32,
    out_width: *mut i32,
) {
    *out_width = (*font_obj).width as i16 as i32;
    *out_total = 0;
    let mut p = text;
    loop {
        let ch = *p;
        if ch == 0 {
            break;
        }
        let glyph_idx = *(*font_obj).char_to_glyph_idx.add(ch as usize);
        let advance = if glyph_idx == 0 {
            (*font_obj).width_div_5 as i16 as i32
        } else {
            glyph_advance_metric(font_obj, glyph_idx)
        };
        *out_total += advance;
        p = p.add(1);
    }
}

/// Port of `FUN_004f9ad0` — extends an existing `FontObject` with new glyphs
/// loaded from a `.fex` (font extension) file.
///
/// Original convention: usercall with `EAX = filename`, `ESI = font_obj`,
/// stack args = (`layer_ctx`, `char_map`, `palette_value`), `RET 0xC`.
///
/// The `.fex` file contains raw pixel data for new glyphs (no header). Each
/// glyph occupies `font.width * stride` bytes (where `stride = font.width`,
/// or `font.width + 1` for fonts with `width >= 16`). The new bitmap also
/// gets remapped through a 256-byte LUT built by walking R/G/B in lockstep
/// with the per-channel steps packed into `palette_value` and using
/// `palette_find_nearest_cached` to map each composed RGB to a palette index.
///
/// On success the function:
/// - allocates a single new buffer holding `[old + new glyph entries][new pixels]`
/// - copies the old glyph entries verbatim into the front of the new buffer
/// - writes per-codepoint indices into the font's char-width table (the bytes
///   referenced by `font_obj.char_to_glyph_idx`)
/// - reads the new pixel rows from disk via `_fread`
/// - computes a tight bounding box per new glyph and writes it as a new entry
/// - remaps every new pixel byte through the LUT
/// - swaps `font_obj.glyph_table` to the new buffer and bumps `font_obj.height`
/// - records the allocation in `font_obj.aux_alloc`
///
/// The `pixel_offset` field in each new glyph entry stores
/// `(absolute_pixel_addr - font_obj.pixel_data)` so the existing
/// `Font__DrawText` lookup `pixel_data + offset` still resolves correctly,
/// even though the new pixels live in a separate allocation.
///
/// # Safety
/// `font_obj` must be a fully-parsed `FontObject` (created by `load_font`).
/// `palette_ctx` must be a valid `PaletteContext`. `filename` and `char_map`
/// must be valid null-terminated C strings.
pub unsafe fn font_extend(
    font_obj: *mut Font,
    palette_ctx: *mut PaletteContext,
    filename: *const c_char,
    char_map: *const c_char,
    palette_value: u32,
) {
    use crate::address::va;
    use crate::rebase::rb;
    use crate::render::palette::{palette_find_nearest_cached, remap_pixels_through_lut};
    use crate::wa_alloc::wa_malloc;
    use core::ffi::c_char;

    let char_map = char_map as *const u8;

    // Open the .fex file via WA's CRT _fopen.
    let fopen: unsafe extern "cdecl" fn(*const c_char, *const c_char) -> *mut u8 =
        core::mem::transmute(rb(va::WA_FOPEN) as usize);
    let fread: unsafe extern "cdecl" fn(*mut u8, u32, u32, *mut u8) -> u32 =
        core::mem::transmute(rb(va::WA_FREAD) as usize);
    let fclose: unsafe extern "cdecl" fn(*mut u8) -> i32 =
        core::mem::transmute(rb(va::WA_FCLOSE) as usize);

    let file = fopen(filename, c"rb".as_ptr());
    if file.is_null() {
        return;
    }

    // ── Geometry ──
    let font_width = (*font_obj).width as i32;
    // Per-glyph row stride in the .fex file. Wide fonts use width+1 to allow
    // an extra byte after each row (matches the `if (font.width > 15)` test).
    let row_stride = if font_width > 15 {
        font_width + 1
    } else {
        font_width
    };

    // strlen(char_map) — count of new glyphs.
    let mut new_count: i32 = 0;
    let mut p = char_map;
    while *p != 0 {
        p = p.add(1);
        new_count += 1;
    }

    let old_height = (*font_obj).height as i32; // existing glyph count
    let total_glyphs = old_height + new_count; // resulting glyph count
    let glyph_entries_bytes = total_glyphs * 12;
    let new_pixels_bytes = new_count * font_width * row_stride;
    let total_size = (glyph_entries_bytes + new_pixels_bytes) as u32;

    // Allocate (matches WA_MallocMemset alignment + 0x20 guard).
    let alloc = wa_malloc(((total_size + 3) & !3u32).wrapping_add(0x20));
    if alloc.is_null() {
        let _ = fclose(file);
        return;
    }
    core::ptr::write_bytes(alloc, 0, total_size as usize);

    // pixel data area starts immediately after the glyph entries.
    let new_pixels = alloc.add(glyph_entries_bytes as usize);

    // Record the allocation so font cleanup paths can free it.
    (*font_obj).aux_alloc = alloc as u32;

    // ── Copy existing glyph entries verbatim into the new buffer ──
    let old_glyphs = (*font_obj).glyph_table as *const u8;
    if old_height > 0 && !old_glyphs.is_null() {
        core::ptr::copy_nonoverlapping(old_glyphs, alloc, (old_height * 12) as usize);
    }

    // ── Update the per-codepoint char→glyph-index table ──
    // For each new char in char_map, write the new (1-based) glyph index
    // `(old_height + idx + 1)` into the table at `font_obj.char_to_glyph_idx`.
    let char_to_glyph = (*font_obj).char_to_glyph_idx;
    let mut q = char_map;
    for i in 0..new_count {
        let ch = *q as usize;
        q = q.add(1);
        *char_to_glyph.add(ch) = (old_height + i + 1) as u8;
    }

    // ── Build 256-byte palette LUT by stepping R/G/B per `palette_value` ──
    // The original walks 256 entries, accumulating signed R/G/B values via the
    // step bytes packed in `palette_value` (low byte=R step, mid=G, high=B),
    // each reduced mod 255 with sign correction (the `IMUL 0x80808081` trick).
    // Each composed RGB is fed to `palette_find_nearest_cached` and the
    // returned palette index becomes lut[i].
    let r_step = (palette_value & 0xff) as i32;
    let g_step = ((palette_value >> 8) & 0xff) as i32;
    let b_step = ((palette_value >> 16) & 0xff) as i32;

    let mut palette_lut = [0u8; 256];
    let mut acc_r: i32 = 0;
    let mut acc_g: i32 = 0;
    let mut acc_b: i32 = 0;
    for slot in palette_lut.iter_mut() {
        // Reduce each accumulator mod 255 with sign correction. The original
        // uses signed magic division by 255; for 0..255 the result is just
        // `acc - 0` for 0..254 and `acc - 1` for 255. We use rem_euclid for
        // a clean cross-platform equivalent (palette steps are non-negative
        // so this matches the original even on edge cases).
        let r = mod_255_signed(acc_r);
        let g = mod_255_signed(acc_g);
        let b = mod_255_signed(acc_b);
        let composed = (r as u32) | ((g as u32) << 8) | ((b as u32) << 16);

        let mut distance: i32 = 0;
        let idx = palette_find_nearest_cached(palette_ctx, composed, &mut distance) as u8;
        *slot = idx;

        acc_r += r_step;
        acc_g += g_step;
        acc_b += b_step;
    }

    // ── Read the new pixel rows from disk ──
    // fread(new_pixels, new_count, font_width * row_stride, file)
    let _ = fread(
        new_pixels,
        new_count as u32,
        (font_width * row_stride) as u32,
        file,
    );
    let _ = fclose(file);

    // ── Bounding-box scan for each new glyph + write its glyph entry ──
    let mut row_offset_in_pixels: i32 = 0; // i.e. iVar16 in the original
    let new_glyph_base = alloc as *mut GlyphEntry;
    for new_idx in 0..new_count {
        // Find the LEFTMOST non-empty column (== start_x).
        let mut start_x: u16 = 0;
        let mut found_left = false;
        'outer_left: for col in 0..font_width as u16 {
            for row in 0..font_width as i32 {
                let addr = new_pixels
                    .offset(((row + row_offset_in_pixels) * font_width + col as i32) as isize);
                if *addr != 0 {
                    start_x = col;
                    found_left = true;
                    break 'outer_left;
                }
            }
        }
        // Find the TOPMOST non-empty row (== start_y).
        let mut start_y: u16 = 0;
        if found_left {
            'outer_top: for row in 0..font_width as u16 {
                for col in 0..font_width as i32 {
                    let addr = new_pixels
                        .offset((col + (row as i32 + row_offset_in_pixels) * font_width) as isize);
                    if *addr != 0 {
                        start_y = row;
                        break 'outer_top;
                    }
                }
            }
        }
        // Find the RIGHTMOST non-empty column (scan right→left from font_width-1).
        let mut right: u16 = (font_width - 1).max(0) as u16;
        if found_left && right != 0 {
            'outer_right: for col in (1..font_width as u16).rev() {
                let mut empty = true;
                for row in 0..font_width as i32 {
                    let addr = new_pixels
                        .offset(((row + row_offset_in_pixels) * font_width + col as i32) as isize);
                    if *addr != 0 {
                        empty = false;
                        break;
                    }
                }
                if !empty {
                    right = col;
                    break 'outer_right;
                }
                right = col - 1;
            }
        }
        // Find the BOTTOMMOST non-empty row (scan bottom→top from font_width-1).
        let mut bottom: u16 = (font_width - 1).max(0) as u16;
        if found_left {
            loop {
                let mut empty = true;
                for col in 0..font_width as i32 {
                    let addr = new_pixels.offset(
                        (col + (bottom as i32 + row_offset_in_pixels) * font_width) as isize,
                    );
                    if *addr != 0 {
                        empty = false;
                        break;
                    }
                }
                if !empty || bottom == 0 {
                    break;
                }
                bottom -= 1;
            }
        }

        // Write the new glyph entry. The slot index is (old_height + new_idx).
        let entry = new_glyph_base.add((old_height + new_idx) as usize);
        (*entry).start_x = start_x;
        (*entry).start_y = start_y;
        (*entry).width = right.saturating_sub(start_x) + 1;
        (*entry).height = bottom.saturating_sub(start_y) + 1;
        // pixel_offset = absolute address of the glyph's bitmap top-left,
        // expressed as a delta from font_obj.pixel_data so existing draw code
        // can do `pixel_data + offset` and reach the new allocation.
        let abs_addr = new_pixels.offset(
            (start_x as i32 + (start_y as i32 + row_offset_in_pixels) * font_width) as isize,
        );
        (*entry).pixel_offset = (abs_addr as u32).wrapping_sub((*font_obj).pixel_data as u32);

        // Advance to the next glyph's pixel rows.
        row_offset_in_pixels += row_stride;
    }

    // ── Remap every new pixel byte through the LUT ──
    let total_pixel_bytes = font_width * new_count * row_stride;
    if total_pixel_bytes > 0 {
        // remap_pixels_through_lut takes (data, pitch, lut, width_dwords, height).
        // The original walks each byte one at a time, so use width_dwords = 1
        // and height = total_bytes / 4 ... actually simpler: do a flat single-row
        // remap with width_dwords = total/4 and height = 1.
        let dword_count = (total_pixel_bytes as u32 + 3) / 4;
        remap_pixels_through_lut(new_pixels, 0, palette_lut.as_ptr(), dword_count, 1);
    }

    // ── Swap glyph_table and bump glyph count ──
    (*font_obj).glyph_table = alloc as *mut GlyphEntry;
    (*font_obj).height = (old_height + new_count) as u16;
}

/// Helper for `font_extend`'s palette stepping. Reduces a non-negative i32
/// modulo 255 the way the original signed-magic-division sequence does.
#[inline]
fn mod_255_signed(value: i32) -> i32 {
    // For non-negative values this is just `value % 255`.
    // The original uses signed magic-division so we mirror that for safety.
    let q = value / 255;
    value - q * 255
}

/// Pure-Rust port of `Font__SetPalette` (0x4F9F20).
///
/// **The function name is misleading** — despite "SetPalette", this routine
/// extends the font with two new derived glyphs rather than recoloring an
/// existing palette. It is called once during `DDGame__LoadFonts` for the
/// digital seven-segment font (`digiwht.fnt`, slot 28), which doesn't ship
/// with `'.'` or `';'` glyphs and needs them generated at runtime.
///
/// What it does, in order:
///
/// 1. Look up the **`'-'`** (0x2D), **`'8'`** (0x38) and **`':'`** (0x3A)
///    glyph indices via `char_to_glyph_idx`. If any is missing, return 0.
/// 2. Sample the foreground-color palette indices of `'-'` and `'8'` by
///    reading byte +5 within each glyph's pixel data. Save them as
///    `minus_fg` and `eight_fg`.
/// 3. Allocate a new buffer sized for `(old_height + 2)` glyph entries
///    plus enough pixel area for **two** glyphs the size of `':'`.
/// 4. Copy all existing glyph entries verbatim into the start of the
///    buffer.
/// 5. Add two new glyph entries:
///    - `char_to_glyph_idx[0x2E]` (= `'.'`) → 1-based index `old_height + 1`
///    - `char_to_glyph_idx[0x3B]` (= `';'`) → 1-based index `old_height + 2`
///
///    Both new entries copy `start_x`/`start_y`/`width`/`height` from the
///    `':'` glyph. Their `pixel_offset` fields point at consecutive
///    `colon_height * font_width`-byte regions inside the new buffer.
/// 6. Initialize the **first** new glyph (`'.'`)'s pixel area:
///    - `memset(area, palette_value as u8, colon_height * font_width / 2)`
///      — fills approximately the top half with a uniform palette index.
///    - For rows `colon_height/2 .. colon_height-1`, copy `colon.width`
///      bytes per row from `':'`'s pixel data into the new buffer at
///      `font_width`-strided rows.
/// 7. Initialize the **second** new glyph (`';'`)'s pixel area: for every
///    `(row, col)` in `(0..colon.height, 0..colon.width)`, copy the
///    `':'` source pixel; if it equals `eight_fg`, replace it with
///    `minus_fg` instead.
/// 8. Update `font_obj.glyph_table` to point at the new buffer, store the
///    new buffer in `aux_alloc` (so destructors can free it), and bump
///    `font_obj.height` by 2.
///
/// The original uses `wa_malloc(round_up_to_4(total_size) + 0x20)` and
/// zero-initializes the body up to `total_size` (the trailing 0x20 padding
/// is left uninitialized).
///
/// # Safety
/// `font_obj` must be a valid mutable `FontObject`. `palette_value`'s low
/// byte is used as a single-byte memset filler. The original's
/// allocate/copy/update sequence is preserved verbatim.
pub unsafe fn font_set_palette_impl(font_obj: *mut Font, palette_value: u32) -> u32 {
    use crate::wa_alloc::wa_malloc;

    let char_to_glyph = (*font_obj).char_to_glyph_idx;
    if char_to_glyph.is_null() {
        return 0;
    }

    // Step 1: '-' (0x2D) glyph index.
    let minus_idx = *char_to_glyph.add(0x2D);
    if minus_idx == 0 {
        return 0;
    }

    // Step 2a: read byte +5 of '-' glyph's pixel data.
    let glyph_table = (*font_obj).glyph_table;
    let pixel_data = (*font_obj).pixel_data;
    let minus_glyph = glyph_table.add(minus_idx as usize - 1);
    let minus_fg = *pixel_data.add((*minus_glyph).pixel_offset as usize + 5);

    // Step 2b: '8' (0x38) glyph index + byte +5.
    let eight_idx = *char_to_glyph.add(0x38);
    if eight_idx == 0 {
        return 0;
    }
    let eight_glyph = glyph_table.add(eight_idx as usize - 1);
    let eight_fg = *pixel_data.add((*eight_glyph).pixel_offset as usize + 5);

    // Step 1c: ':' (0x3A) — required for the template.
    let colon_idx = *char_to_glyph.add(0x3A);
    if colon_idx == 0 {
        return 0;
    }

    let old_height = (*font_obj).height as i32; // signed via MOVSX in the original
    let new_height = old_height + 2;
    let font_width = (*font_obj).width as i16 as i32;

    // Step 3: compute total size and allocate.
    //   total_size = (colon.height * font_width + new_height * 6) * 2
    //              = colon_height * font_width * 2  +  new_height * 12
    //   alloc = round_up_to_dword(total_size) + 0x20
    let colon_glyph = glyph_table.add(colon_idx as usize - 1);
    let colon_height = (*colon_glyph).height as i32;
    let colon_width = (*colon_glyph).width as i32;
    let colon_pixel_offset = (*colon_glyph).pixel_offset as usize;

    let total_size = (colon_height * font_width + new_height * 6) * 2;
    let alloc_size = ((total_size as u32 + 3) & !3u32) + 0x20;
    let new_buffer = wa_malloc(alloc_size);
    if new_buffer.is_null() {
        return 0;
    }
    // Zero-init the body (not the trailing 0x20 padding).
    core::ptr::write_bytes(new_buffer, 0, total_size as usize);

    // _Dst = new_buffer + new_height*12 = where pixel data starts in the new buffer.
    let new_glyph_table = new_buffer as *mut GlyphEntry;
    let new_pixel_area = new_buffer.add((new_height as usize) * 12);

    // Save the allocation in aux_alloc so cleanup paths can free it
    // (the original writes this *before* the copy loop).
    (*font_obj).aux_alloc = new_buffer as u32;

    // Step 4: copy existing glyph entries.
    if old_height > 0 {
        core::ptr::copy_nonoverlapping(
            glyph_table as *const u8,
            new_buffer,
            (old_height as usize) * 12,
        );
    }

    // Step 5: write new char_to_glyph_idx entries for '.' and ';'.
    *char_to_glyph.add(0x2E) = (old_height as u8).wrapping_add(1);
    *char_to_glyph.add(0x3B) = (old_height as u8).wrapping_add(2);

    // Step 5b: first new glyph (= colon copy with adjusted pixel_offset).
    //   pixel_offset = (_Dst - pixel_data)
    let new_first = new_glyph_table.add(old_height as usize);
    (*new_first).start_x = (*colon_glyph).start_x;
    (*new_first).start_y = (*colon_glyph).start_y;
    (*new_first).width = (*colon_glyph).width;
    (*new_first).height = (*colon_glyph).height;
    (*new_first).pixel_offset = (new_pixel_area as u32).wrapping_sub(pixel_data as u32);

    // Step 5c: second new glyph (= duplicate of first new glyph, but its
    // pixel_offset is bumped by colon_height * font_width so it points to
    // the *next* chunk of new pixel data).
    let new_second = new_first.add(1);
    (*new_second).start_x = (*new_first).start_x;
    (*new_second).start_y = (*new_first).start_y;
    (*new_second).width = (*new_first).width;
    (*new_second).height = (*new_first).height;
    (*new_second).pixel_offset = (*new_first)
        .pixel_offset
        .wrapping_add((colon_height * font_width) as u32);

    // Step 6a: memset top portion of FIRST new glyph with palette_value's
    // low byte. Length = (colon_height * font_width) with sign-correct /2.
    let memset_len = {
        let v = colon_height.wrapping_mul(font_width);
        // CDQ + SUB EAX,EDX + SAR EAX,1 = signed div by 2 with rounding toward zero
        let v_signed = v;
        let cdq = v_signed >> 31;
        ((v_signed - cdq) >> 1) as usize
    };
    core::ptr::write_bytes(new_pixel_area, palette_value as u8, memset_len);

    // Step 6b: copy bottom half rows of ':' into FIRST new glyph at
    // font-width row stride.
    let half = (colon_height as u32 >> 1) as i32;
    if half < colon_height {
        for row in half..colon_height {
            let src = pixel_data.add(colon_pixel_offset + (row * colon_width) as usize);
            let dst = new_pixel_area.add((row * font_width) as usize);
            core::ptr::copy_nonoverlapping(src, dst, colon_width as usize);
        }
    }

    // Step 7: pixel-by-pixel copy for SECOND new glyph with eight→minus
    // foreground substitution. The destination is offset by
    // colon_height * font_width within new_pixel_area.
    if colon_height > 0 {
        for row in 0..colon_height {
            for col in 0..colon_width {
                let src_addr =
                    pixel_data.add(colon_pixel_offset + (row * colon_width + col) as usize);
                let dst_addr =
                    new_pixel_area.add(((colon_height + row) * font_width + col) as usize);
                let src_byte = *src_addr;
                if src_byte == eight_fg {
                    *dst_addr = minus_fg;
                } else {
                    *dst_addr = src_byte;
                }
            }
        }
    }

    // Step 8: update font_obj fields.
    (*font_obj).glyph_table = new_glyph_table;
    (*font_obj).height = new_height as u16;
    1
}
