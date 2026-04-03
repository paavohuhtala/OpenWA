//! Sprite loading hook replacements.
//!
//! Replaces ConstructSprite (0x4FAA30) and ProcessSprite (0x4FAB80) with
//! Rust implementations. Uses `parse_spr_header` from openwa-core for
//! format parsing.

use openwa_core::address::va;
use openwa_core::rebase::rb;
use openwa_core::render::palette::PaletteContext;
use openwa_core::render::sprite::{Sprite, SpriteFrame};

use crate::hook::{self, usercall_trampoline};

// ---------------------------------------------------------------------------
// ConstructSprite (0x4FAA30) — usercall(EAX=sprite, ECX=context), plain RET
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_construct_sprite; impl_fn = construct_sprite_impl;
    regs = [eax, ecx]);

unsafe extern "cdecl" fn construct_sprite_impl(sprite: *mut Sprite, context: *mut u8) {
    // Zero the entire 0x70-byte struct first
    core::ptr::write_bytes(sprite as *mut u8, 0, core::mem::size_of::<Sprite>());

    (*sprite).vtable = rb(va::SPRITE_VTABLE) as *mut u8;
    (*sprite).context_ptr = context;
    (*sprite).display_gfx = rb(va::BIT_GRID_DISPLAY_VTABLE) as *mut u8;

    // DisplayGfx sub-object fields (within _unknown_38)
    let p = sprite as *mut u8;
    *(p.add(0x38) as *mut u32) = 1;
    *(p.add(0x40) as *mut u32) = 8;
}

// ---------------------------------------------------------------------------
// ProcessSprite (0x4FAB80)
// usercall(EAX=sprite, ECX=palette_ctx) + 1 stack(raw_data), RET 0x4
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_process_sprite; impl_fn = process_sprite_impl;
    regs = [eax, ecx]; stack_params = 1; ret_bytes = "0x4");

// ---------------------------------------------------------------------------
// PaletteContext__MapColor (0x5412B0)
// thiscall(ECX=palette_ctx, stack=rgb_u32), RET 0x4
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_palette_map_color; impl_fn = palette_map_color_impl;
    reg = ecx; stack_params = 1; ret_bytes = "0x4"; preserve_ecx);

unsafe extern "cdecl" fn palette_map_color_impl(palette_ctx: u32, rgb: u32) -> u32 {
    let ctx = palette_ctx as *mut openwa_core::render::palette::PaletteContext;
    openwa_core::render::palette::palette_map_color(ctx, rgb)
}

unsafe extern "cdecl" fn process_sprite_impl(
    sprite: *mut Sprite,
    palette_ctx: *mut PaletteContext,
    raw_data: *const u8,
) -> u32 {
    use openwa_core::render::palette::palette_map_color;
    use openwa_core::render::spr::parse_spr_header;

    // We need the data as a slice. data_size is at raw_data + 4.
    let data_size = *(raw_data.add(4) as *const u32);
    // Total buffer: data_size covers from offset +4 onward, so total = data_size + 4.
    let data_len = (data_size + 4) as usize;
    let data = core::slice::from_raw_parts(raw_data, data_len);

    let hdr = match parse_spr_header(data) {
        Ok(h) => h,
        Err(e) => {
            panic!("ProcessSprite: failed to parse .spr data: {}", e);
        }
    };

    // --- Update global counter: data_size ---
    let g_data_bytes = rb(va::G_SPRITE_DATA_BYTES) as *mut u32;
    *g_data_bytes = (*g_data_bytes).wrapping_add(data_size);

    // --- Store raw_frame_header_ptr (points to header_flags in raw buffer) ---
    (*sprite).raw_frame_header_ptr = raw_data.add(8) as *mut u8;

    // --- Store header_flags ---
    (*sprite).header_flags = hdr.header_flags;

    // --- Build palette lookup table IN PLACE ---
    // palette_data_ptr points to raw_data + 0x0A (the palette_entry_count field).
    // WA overwrites this region with a 1-indexed palette index lookup table:
    //   [0] = 0 (transparent)
    //   [1..N] = PaletteContext__MapColor(rgb) for each entry
    let palette_base = raw_data.add(0x0A) as *mut u8;
    (*sprite).palette_data_ptr = palette_base;

    // Update global counter: palette bytes
    let g_palette = rb(va::G_SPRITE_PALETTE_BYTES) as *mut u32;
    *g_palette = (*g_palette).wrapping_add(hdr.palette_count as u32 * 3);

    // Write transparent index
    *palette_base = 0;

    // Map each RGB entry to display palette index
    let rgb_start = raw_data.add(hdr.palette_offset);
    for i in 0..hdr.palette_count as usize {
        // Read 4 bytes (3 RGB + 1 from next entry, matching WA behavior)
        let rgb_val = *(rgb_start.add(i * 3) as *const u32);
        let mapped = palette_map_color(palette_ctx, rgb_val);
        *palette_base.add(1 + i) = mapped as u8;
    }

    // --- Secondary frame table (if header_flags & 0x4000) ---
    let has_secondary = hdr.header_flags & 0x4000 != 0;
    if has_secondary {
        (*sprite).secondary_frame_count = hdr.secondary_frame_count;
        (*sprite).secondary_frame_ptr =
            raw_data.add(hdr.secondary_frame_offset) as *mut SpriteFrame;
    }

    // --- Main frame header fields ---
    // Copy unknown_08 + fps as a single u32 (matching WA's 4-byte copy)
    let frame_header_ptr = if has_secondary {
        raw_data.add(hdr.secondary_frame_offset + hdr.secondary_frame_count as usize * 12)
    } else {
        raw_data.add(hdr.palette_offset + hdr.palette_count as usize * 3)
    };

    // WA copies 4 bytes at once: *(u32*)(sprite+8) = *(u32*)(frame_header)
    let p = sprite as *mut u8;
    *(p.add(0x08) as *mut u32) = *(frame_header_ptr as *const u32);
    (*sprite).flags = hdr.flags;
    (*sprite).width = hdr.width;
    (*sprite).height = hdr.height;
    (*sprite).frame_count = hdr.frame_count;
    (*sprite).max_frames = hdr.max_frames;

    // --- Scale fields ---
    if hdr.is_scaled {
        (*sprite).scale_x = hdr.scale_x;
        (*sprite).scale_y = hdr.scale_y;
        (*sprite).is_scaled = 1;
    } else {
        (*sprite).is_scaled = 0;
    }

    // --- Frame metadata and bitmap pointers ---
    let frame_meta_ptr = raw_data.add(hdr.frame_meta_offset) as *mut SpriteFrame;
    let bitmap_ptr = raw_data.add(hdr.bitmap_offset) as *mut u8;
    (*sprite).frame_meta_ptr = frame_meta_ptr;
    (*sprite).bitmap_data_ptr = bitmap_ptr;

    // --- Bitmap palette remapping (only when NO secondary frames) ---
    if !has_secondary {
        // Remap every bitmap byte: pixel = lookup_table[pixel]
        let bitmap_byte_count_raw = (data_size as usize + 4).saturating_sub(hdr.bitmap_offset);
        let dword_count = (bitmap_byte_count_raw + 3) / 4;
        let remap_byte_count = dword_count * 4;

        for i in 0..remap_byte_count {
            let idx = *bitmap_ptr.add(i) as usize;
            *bitmap_ptr.add(i) = *palette_base.add(idx);
        }
    }

    // --- Update global counters: pixel area and frame count ---
    if hdr.frame_count > 0 {
        let g_pixel_area = rb(va::G_SPRITE_PIXEL_AREA) as *mut u32;
        for i in 0..hdr.frame_count as usize {
            let frame = &*frame_meta_ptr.add(i);
            let area = (frame.end_x as i32 - frame.start_x as i32)
                * (frame.end_y as i32 - frame.start_y as i32);
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

        let _ = hook::install(
            "PaletteContext__MapColor",
            va::PALETTE_CONTEXT_MAP_COLOR,
            trampoline_palette_map_color as *const (),
        )?;
    }
    Ok(())
}
