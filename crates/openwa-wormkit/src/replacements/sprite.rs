//! Sprite loading hook replacements.
//!
//! Replaces ConstructSprite (0x4FAA30) and ProcessSprite (0x4FAB80) with
//! Rust implementations. Uses `parse_spr_header` from openwa-core for
//! format parsing.

use openwa_core::address::va;
use openwa_core::rebase::rb;

use crate::hook::{self, usercall_trampoline};

// ---------------------------------------------------------------------------
// ConstructSprite (0x4FAA30) — usercall(EAX=sprite, ECX=context), plain RET
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_construct_sprite; impl_fn = construct_sprite_impl;
    regs = [eax, ecx]);

unsafe extern "cdecl" fn construct_sprite_impl(sprite: u32, context: u32) {
    let p = sprite as *mut u8;

    // Zero the entire 0x70-byte struct first
    core::ptr::write_bytes(p, 0, 0x70);

    // Vtable
    *(p as *mut u32) = rb(va::SPRITE_VTABLE);
    // Context pointer (+0x04)
    *(p.add(0x04) as *mut u32) = context;
    // DisplayGfx vtable (+0x34)
    *(p.add(0x34) as *mut u32) = rb(va::DISPLAYGFX_VTABLE);
    // _unknown_38 = 1 (+0x38)
    *(p.add(0x38) as *mut u32) = 1;
    // _unknown_40 = 8 (+0x40)
    *(p.add(0x40) as *mut u32) = 8;
}

// ---------------------------------------------------------------------------
// ProcessSprite (0x4FAB80)
// usercall(EAX=sprite, ECX=palette_ctx) + 1 stack(raw_data), RET 0x4
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_process_sprite; impl_fn = process_sprite_impl;
    regs = [eax, ecx]; stack_params = 1; ret_bytes = "0x4");

static mut PALETTE_MAP_COLOR_ADDR: u32 = 0;

/// Call WA's PaletteContext__MapColor to find the nearest display palette index.
#[cfg(target_arch = "x86")]
unsafe fn palette_map_color(palette_ctx: u32, rgb: u32) -> u8 {
    let f: unsafe extern "thiscall" fn(u32, u32) -> u32 =
        core::mem::transmute(PALETTE_MAP_COLOR_ADDR as usize);
    f(palette_ctx, rgb) as u8
}

unsafe extern "cdecl" fn process_sprite_impl(
    sprite: u32,
    palette_ctx: u32,
    raw_data: u32,
) -> u32 {
    use openwa_core::render::spr::parse_spr_header;

    let p = sprite as *mut u8;
    let data_ptr = raw_data as *const u8;

    // We need the data as a slice. Use data_size from header to determine length.
    // data_size is at raw_data + 4.
    let data_size = *(data_ptr.add(4) as *const u32);
    // Total buffer: data_size covers from offset +4 onward, so total = data_size + 4.
    let data_len = (data_size + 4) as usize;
    let data = core::slice::from_raw_parts(data_ptr, data_len);

    let hdr = match parse_spr_header(data) {
        Ok(h) => h,
        Err(e) => {
            // This should never happen with valid WA game data
            panic!("ProcessSprite: failed to parse .spr data: {}", e);
        }
    };

    // --- Update global counter: data_size ---
    let g_data_bytes = rb(va::G_SPRITE_DATA_BYTES) as *mut u32;
    *g_data_bytes = (*g_data_bytes).wrapping_add(data_size);

    // --- Store raw_frame_header_ptr (points to header_flags in raw buffer) ---
    *(p.add(0x60) as *mut *const u8) = data_ptr.add(8);

    // --- Store header_flags ---
    *(p.add(0x14) as *mut u16) = hdr.header_flags;

    // --- Build palette lookup table IN PLACE ---
    // palette_data_ptr points to raw_data + 0x0A (the palette_entry_count field).
    // WA overwrites this region with a 1-indexed palette index lookup table:
    //   [0] = 0 (transparent)
    //   [1..N] = PaletteContext__MapColor(rgb) for each entry
    let palette_base = data_ptr.add(0x0A) as *mut u8;
    *(p.add(0x68) as *mut *mut u8) = palette_base;

    // Update global counter: palette bytes
    let g_palette = rb(va::G_SPRITE_PALETTE_BYTES) as *mut u32;
    *g_palette = (*g_palette).wrapping_add(hdr.palette_count as u32 * 3);

    // Write transparent index
    *palette_base = 0;

    // Map each RGB entry to display palette index
    let rgb_start = data_ptr.add(hdr.palette_offset);
    for i in 0..hdr.palette_count as usize {
        // Read 4 bytes (3 RGB + 1 from next entry, matching WA behavior)
        let rgb_val = *(rgb_start.add(i * 3) as *const u32);
        let mapped = palette_map_color(palette_ctx, rgb_val);
        *palette_base.add(1 + i) = mapped;
    }

    // --- Secondary frame table (if header_flags & 0x4000) ---
    let has_secondary = hdr.header_flags & 0x4000 != 0;
    if has_secondary {
        *(p.add(0x30) as *mut u16) = hdr.secondary_frame_count;
        *(p.add(0x2C) as *mut *const u8) = data_ptr.add(hdr.secondary_frame_offset);
    }

    // --- Main frame header fields ---
    // Copy unknown_08 + fps as a single u32 (matching WA's 4-byte copy)
    let frame_header_ptr = if has_secondary {
        // After secondary frames
        data_ptr.add(hdr.secondary_frame_offset + hdr.secondary_frame_count as usize * 12)
    } else {
        data_ptr.add(hdr.palette_offset + hdr.palette_count as usize * 3)
    };

    // WA copies 4 bytes at once: *(u32*)(sprite+8) = *(u32*)(frame_header)
    *(p.add(0x08) as *mut u32) = *(frame_header_ptr as *const u32);
    *(p.add(0x10) as *mut u16) = hdr.flags;
    *(p.add(0x0C) as *mut u16) = hdr.width;
    *(p.add(0x0E) as *mut u16) = hdr.height;
    *(p.add(0x12) as *mut u16) = hdr.frame_count;
    *(p.add(0x16) as *mut u16) = hdr.max_frames;

    // --- Scale fields ---
    if hdr.is_scaled {
        *(p.add(0x1C) as *mut u32) = hdr.scale_x;
        *(p.add(0x20) as *mut u32) = hdr.scale_y;
        *(p.add(0x24) as *mut u32) = 1; // is_scaled
    } else {
        *(p.add(0x24) as *mut u32) = 0;
    }

    // --- Frame metadata and bitmap pointers ---
    let frame_meta_ptr = data_ptr.add(hdr.frame_meta_offset);
    let bitmap_ptr = data_ptr.add(hdr.bitmap_offset);
    *(p.add(0x28) as *mut *const u8) = frame_meta_ptr;
    *(p.add(0x64) as *mut *const u8) = bitmap_ptr;

    // --- Bitmap palette remapping (only when NO secondary frames) ---
    if !has_secondary {
        // Remap every bitmap byte: pixel = lookup_table[pixel]
        let bitmap_start_relative = hdr.bitmap_offset;
        let bitmap_byte_count_raw = (data_size as usize + 4).saturating_sub(bitmap_start_relative);
        let dword_count = (bitmap_byte_count_raw + 3) / 4;
        let remap_byte_count = dword_count * 4;

        let bmp = bitmap_ptr as *mut u8;
        for i in 0..remap_byte_count {
            let idx = *bmp.add(i) as usize;
            *bmp.add(i) = *palette_base.add(idx);
        }
    }

    // --- Update global counters: pixel area and frame count ---
    if hdr.frame_count > 0 {
        let g_pixel_area = rb(va::G_SPRITE_PIXEL_AREA) as *mut u32;
        let frames_ptr = frame_meta_ptr as *const [u8; 12];
        for i in 0..hdr.frame_count as usize {
            let frame = &*frames_ptr.add(i);
            let start_x = i16::from_le_bytes([frame[4], frame[5]]);
            let start_y = i16::from_le_bytes([frame[6], frame[7]]);
            let end_x = i16::from_le_bytes([frame[8], frame[9]]);
            let end_y = i16::from_le_bytes([frame[10], frame[11]]);
            let area = (end_x as i32 - start_x as i32) * (end_y as i32 - start_y as i32);
            *g_pixel_area = (*g_pixel_area).wrapping_add(area as u32);
        }
    }

    let g_frame_count = rb(va::G_SPRITE_FRAME_COUNT) as *mut u32;
    *g_frame_count = (*g_frame_count).wrapping_add(hdr.frame_count as u32);

    1 // success
}

// ---------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------

pub fn install() -> Result<(), String> {
    unsafe {
        PALETTE_MAP_COLOR_ADDR = rb(va::PALETTE_CONTEXT_MAP_COLOR);

        let _ = hook::install(
            "ConstructSprite",
            va::CONSTRUCT_SPRITE,
            trampoline_construct_sprite as *const (),
        )?;

        let _ = hook::install(
            "ProcessSprite",
            va::PROCESS_SPRITE,
            trampoline_process_sprite as *const (),
        )?;
    }
    Ok(())
}
