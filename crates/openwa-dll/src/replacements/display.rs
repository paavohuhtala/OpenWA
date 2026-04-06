//! Display subsystem patches.
//!
//! Patches DisplayBase vtables in WA.exe's .rdata:
//! - Primary vtable (0x6645F8): replaces _purecall slots with safe no-op stubs
//! - Headless vtable (0x66A0F8): replaces destructor with Rust version that
//!   correctly frees our Rust-allocated sprite cache sub-objects

use crate::log_line;
use openwa_core::address::va;
use openwa_core::bitgrid::DisplayBitGrid;
use openwa_core::display::display_vtable::{self as display_vtable, DisplayVtable};
use openwa_core::display::DisplayBase;
use openwa_core::display::DisplayGfx;
use openwa_core::fixed::Fixed;
use openwa_core::rebase::rb;
use openwa_core::vtable::patch_vtable;
use openwa_core::vtable_replace;
use openwa_core::wa_alloc::wa_free;

/// The _purecall function address (calls abort).
const PURECALL: u32 = 0x005D_4E16;

/// Number of slots in the DisplayBase vtable.
const VTABLE_SLOTS: usize = 32;

unsafe extern "thiscall" fn noop_thiscall(_this: *mut u8) {}

/// Rust destructor for headless DisplayBase. Frees the sprite cache chain
/// (wrapper → buffer_ctrl → buffer) that was allocated by new_headless().
unsafe extern "thiscall" fn headless_destructor(
    this: *mut DisplayBase,
    flags: u8,
) -> *mut DisplayBase {
    let sprite_cache = (*this).sprite_cache;
    if !sprite_cache.is_null() {
        let ctrl = (*sprite_cache).buffer_ctrl;
        if !ctrl.is_null() {
            let buf = (*ctrl).buffer;
            if !buf.is_null() {
                wa_free(buf);
            }
            wa_free(ctrl);
        }
        wa_free(sprite_cache);
    }
    if flags & 1 != 0 {
        wa_free(this);
    }
    this
}

// No saved originals needed — all paths are fully ported or use direct bridges.

/// Rust port of DisplayGfx::BlitSprite (slot 19, 0x56B080).
///
/// Standard thiscall: ECX=this, stack params: x, y, sprite_flags, palette (RET 0x10).
///
/// sprite_flags layout:
///   low 16 bits  = sprite ID (0 = no sprite)
///   high 16 bits = orientation/flags:
///     bit 16 (0x10000): tiled mode
///     bit 17: additional orientation
///     bit 18 (0x40000): extra mirror X
///     bit 19 (0x80000): extra mirror Y
///     bit 20 (0x100000): stippled palette adjust
///     bit 21 (0x200000): additive blend
///     bit 22 (0x400000): shadow clear
///     bit 23 (0x800000): invert palette
///     bit 24 (0x1000000): palette ×4 adjust
///     bit 25 (0x2000000): palette transform
///     bit 26 (0x4000000): color blend
///     bit 27 (0x8000000): stippled mode 0
///     bit 28 (0x10000000): stippled mode 1
unsafe extern "thiscall" fn blit_sprite(
    this: *mut DisplayGfx,
    x: Fixed,
    y: Fixed,
    sprite_flags: u32,
    palette: u32,
) {
    use openwa_core::bitgrid::DisplayBitGrid;
    use openwa_core::display::display_vtable;
    use openwa_core::display::gfx::DisplayGfx;

    let gfx = this as *mut DisplayGfx;
    let base = this as *const u8;

    // ---------------------------------------------------------------
    // Extract sprite ID and high flags
    // ---------------------------------------------------------------
    let high_flags = sprite_flags & 0xFFFF_0000;
    let sprite_id = sprite_flags & 0xFFFF;

    if sprite_id == 0 {
        return;
    }

    // ---------------------------------------------------------------
    // Palette manipulation
    // ---------------------------------------------------------------
    let mut pal: u32 = palette;
    if (high_flags & 0x0080_0000) != 0 {
        // Bit 23: invert palette
        pal = 0x10000u32.wrapping_sub(palette);
        if sprite_id.wrapping_sub(0x1D5) < 3 {
            // Special sprite IDs: scale by 8/18
            pal = (0x10000u32.wrapping_sub(palette).wrapping_mul(8)) / 0x12;
        }
    }
    if (high_flags & 0x0200_0000) != 0 {
        // Bit 25: palette transform (modular arithmetic for color cycling)
        let tmp = ((pal.wrapping_mul(0x1F) as i32)
            .wrapping_add(((pal.wrapping_mul(0x1F) as i32) >> 31) & 0x1F)
            >> 5) as u32;
        let tmp = tmp.wrapping_add(0x400) & 0xFFFF;
        pal = (tmp.wrapping_rem(0xF800)) / 2;
        if (pal & 0x400) != 0 {
            pal = (pal & !0x400) | 0x8000;
        }
    }

    // ---------------------------------------------------------------
    // Check sprite arrays — bitmap path if not in primary arrays
    // ---------------------------------------------------------------
    let arr1 = *(base.add(sprite_id as usize * 4 + 0x1008) as *const u32);
    let arr2 = *(base.add(sprite_id as usize * 4 + 0x2008) as *const u32);

    if arr1 == 0 && arr2 == 0 {
        // Bitmap sprite path — sprite is in the bitmap table at 0x3DD4.
        let bitmap_obj = (*gfx).sprite_table[sprite_id as usize];
        if bitmap_obj == 0 {
            return;
        }

        // Get frame data and dimensions from bitmap sprite object
        let mut sprite_w: i32 = 0;
        let mut sprite_h: i32 = 0;
        let mut rect_left: i32 = 0;
        let mut rect_top: i32 = 0;
        let mut rect_right: i32 = 0;
        let mut rect_bottom: i32 = 0;
        let frame_data = wa_get_bitmap_sprite_info(
            bitmap_obj as *mut u8,
            pal,
            &mut sprite_w,
            &mut sprite_h,
            &mut rect_left,
            &mut rect_top,
            &mut rect_right,
            &mut rect_bottom,
            rb(openwa_core::address::va::DISPLAY_GFX_GET_BITMAP_SPRITE_INFO),
        );
        if frame_data.is_null() {
            return;
        }

        let camera_x = (*gfx).camera_x;
        let camera_y = (*gfx).camera_y;
        let half_w = sprite_w / 2;
        let half_h = sprite_h / 2;
        let blit_h = rect_bottom - rect_top;

        let dst_y = (y.0 >> 16) + (camera_y - half_h) + rect_top;

        if (high_flags & 0x0001_0000) == 0 {
            // Non-tiled: BlitBitmapClipped
            let dst_x = (x.0 >> 16) + (camera_x - half_w) + rect_left;
            wa_blit_bitmap_clipped(
                this as *mut u8,
                sprite_w as u32,
                dst_x,
                dst_y,
                blit_h,
                frame_data,
                2,
                rb(openwa_core::address::va::DISPLAY_GFX_BLIT_BITMAP_CLIPPED),
            );
        } else {
            // Tiled: BlitBitmapTiled
            let dst_x = (x.0 >> 16) + (camera_x - half_w) + rect_left;
            wa_blit_bitmap_tiled(
                dst_x,
                sprite_w,
                this as *mut u8,
                dst_y,
                blit_h,
                frame_data,
                rb(openwa_core::address::va::DISPLAY_GFX_BLIT_BITMAP_TILED),
            );
        }
        return;
    }

    // ---------------------------------------------------------------
    // Bit 24: palette ×4 adjust with orientation-dependent high bits
    // ---------------------------------------------------------------
    // The original ASM at 0x56B145 does a complex palette×4 + orientation mapping
    // that writes extra orientation bits into the local orient variable.
    // For now, handle the simple case:
    let mut orient_local: u32 = 0x0000_0001; // blend=1 (ColorTable/transparency), orientation=0 (Normal)
    if (high_flags & 0x0100_0000) != 0 {
        // The ASM computes: pal = pal * 4 + 0x8000, then maps (pal >> 16) & 3
        // to set specific orient values (0x80001, 0xC0001, 0x40001)
        let scaled = pal.wrapping_mul(4).wrapping_add(0x8000);
        pal = scaled & 0xFFFF;
        let quad = ((scaled as i32) >> 16) & 3;
        orient_local = match quad {
            0 => 0x0008_0001,
            1 => 0x000C_0001,
            2 => 0x0004_0001,
            _ => 0x0000_0001, // shouldn't happen, keep default blend=1
        };
    }

    // ---------------------------------------------------------------
    // Sprite data lookup via vtable[33]
    // ---------------------------------------------------------------
    let vtable_ptr = *(this as *const *const u32);
    let slot33_addr = *vtable_ptr.add(33);

    // vtable[33] is thiscall with 9 stack params (RET 0x24).
    // Output semantics (traced from ASM ESP offsets through LEA/PUSH sequence):
    //   param 3 → sprite full width (for centering)
    //   param 4 → sprite full height (for centering)
    //   param 5 → render rect LEFT
    //   param 6 → render rect TOP (overwrites palette on original stack!)
    //   param 7 → render rect RIGHT
    //   param 8 → render rect BOTTOM
    //   param 9 → unknown (unused)
    let mut out_sprite_w: i32 = 0;
    let mut out_sprite_h: i32 = 0;
    let mut out_rect_left: i32 = 0;
    let mut out_rect_top: i32 = 0;
    let mut out_rect_right: i32 = 0;
    let mut out_rect_bottom: i32 = 0;
    let mut out_unknown: u32 = 0;

    let fn33: unsafe extern "thiscall" fn(
        *mut DisplayGfx,
        u32,
        u32,
        *mut i32,
        *mut i32,
        *mut i32,
        *mut i32,
        *mut i32,
        *mut i32,
        *mut u32,
    ) -> *mut DisplayBitGrid = core::mem::transmute(slot33_addr as usize);

    let mut sprite_surface = fn33(
        this,
        sprite_id,
        pal,
        &mut out_sprite_w,
        &mut out_sprite_h,
        &mut out_rect_left,
        &mut out_rect_top,
        &mut out_rect_right,
        &mut out_rect_bottom,
        &mut out_unknown,
    );

    if sprite_surface.is_null() {
        return;
    }

    let sprite_w = out_sprite_w;
    let sprite_h = out_sprite_h;
    let rect_left = out_rect_left;
    let rect_top = out_rect_top;
    let rect_right = out_rect_right;
    let rect_bottom = out_rect_bottom;

    // Size checks
    if rect_left >= rect_right || rect_top >= rect_bottom {
        return;
    }

    let mut blit_w = rect_right - rect_left;
    let mut blit_h = rect_bottom - rect_top;

    // ---------------------------------------------------------------
    // Shadow clear (high_flags bit 22)
    // ---------------------------------------------------------------
    if (high_flags & 0x0040_0000) != 0 {
        // Blit sprite to layer_2 as shadow base
        let layer2 = (*gfx).layer_2;
        super::bitgrid::blit_impl(
            layer2,
            0,
            0,
            blit_w,
            blit_h,
            sprite_surface,
            0,
            0,
            core::ptr::null(),
            0, // mode 0 = copy
        );
        // Manipulate color_add_table entry for shadow
        let color_idx = ((*gfx)._unknown_356c as usize) * 0x100;
        let table_byte = &mut (*gfx).color_add_table[color_idx];
        let saved = *table_byte;
        *table_byte = 0;

        // Call BitGrid__ClearColumn_Maybe (0x4F6590) — clears shadow channel
        let clear_fn: unsafe extern "cdecl" fn(*mut u8) =
            core::mem::transmute(rb(0x004F6590) as usize);
        clear_fn(table_byte as *mut u8);

        *table_byte = saved;

        // Replace sprite surface with layer_2 (shadow-processed)
        sprite_surface = layer2;
    }

    // ---------------------------------------------------------------
    // Extra orientation flags from high_flags
    // ---------------------------------------------------------------
    if (high_flags & 0x0004_0000) != 0 {
        orient_local |= 0x0001_0000;
    }
    if (high_flags & 0x0008_0000) != 0 {
        orient_local |= 0x0002_0000;
    }

    // ---------------------------------------------------------------
    // 16-case orientation switch for camera coordinate mapping
    // ---------------------------------------------------------------
    let camera_x = (*gfx).camera_x;
    let camera_y = (*gfx).camera_y;

    // Signed divide toward zero (matches MSVC CDQ+SUB+SAR pattern)
    let half_w = if sprite_w < 0 {
        (sprite_w + 1) / 2
    } else {
        sprite_w / 2
    };
    let half_h = if sprite_h < 0 {
        (sprite_h + 1) / 2
    } else {
        sprite_h / 2
    };

    let x_px = x.0 >> 16;
    let y_px = y.0 >> 16;

    let (dst_x, dst_y);
    let orientation_key = (orient_local >> 16) as i32;

    match orientation_key {
        1 | 10 => {
            // MirrorX
            dst_x = camera_x + half_w + x_px - rect_right;
            dst_y = camera_y - half_h + rect_top + y_px;
        }
        2 | 9 => {
            // MirrorY — X same as Normal, Y mirrored
            dst_x = camera_x - half_w + rect_left + x_px;
            dst_y = camera_y + half_h + y_px - rect_bottom;
        }
        3 | 8 => {
            // MirrorXY
            dst_x = camera_x + half_w + x_px - rect_right;
            dst_y = camera_y + half_h + y_px - rect_bottom;
        }
        4 | 15 => {
            // Rotate90 — swap axes
            dst_x = camera_x - half_h + rect_top + x_px;
            dst_y = camera_y + half_w + y_px - rect_right;
            blit_w = rect_bottom - rect_top;
            blit_h = rect_right - rect_left;
        }
        5 | 14 => {
            // Rotate90MirrorX
            dst_x = camera_x + half_h + x_px - rect_bottom;
            dst_y = camera_y + half_w + y_px - rect_right;
            blit_w = rect_bottom - rect_top;
            blit_h = rect_right - rect_left;
        }
        6 | 13 => {
            // Rotate90MirrorY
            dst_x = camera_x - half_h + rect_top + x_px;
            dst_y = camera_y - half_w + rect_left + y_px;
            blit_w = rect_bottom - rect_top;
            blit_h = rect_right - rect_left;
        }
        7 | 12 => {
            // Rotate90MirrorXY
            dst_x = camera_x + half_h + x_px - rect_bottom;
            dst_y = camera_y - half_w + rect_left + y_px;
            blit_w = rect_bottom - rect_top;
            blit_h = rect_right - rect_left;
        }
        _ => {
            // Normal (0, 11, and any other value)
            dst_x = camera_x - half_w + rect_left + x_px;
            dst_y = camera_y - half_h + rect_top + y_px;
        }
    }

    // ---------------------------------------------------------------
    // Blit dispatch based on high_flags
    // ---------------------------------------------------------------

    if blit_w <= 0 || blit_h <= 0 {
        return;
    }

    // Stippled mode (checkerboard per-pixel blit)
    if (high_flags & 0x0800_0000) != 0 || (high_flags & 0x1000_0000) != 0 {
        let stipple_mode: u32 = if (high_flags & 0x1000_0000) != 0 {
            1
        } else {
            0
        };
        let parity = *(rb(openwa_core::address::va::G_STIPPLE_PARITY) as *const u32);

        display_vtable::acquire_render_lock(gfx);

        super::bitgrid::blit_stippled_raw(
            (*gfx).layer_0,
            sprite_surface,
            dst_x,
            dst_y,
            blit_w,
            blit_h,
            0,
            0,
            stipple_mode,
            parity,
        );
        return;
    }

    // Tiled mode (horizontal sprite tiling)
    if (high_flags & 0x0001_0000) != 0 {
        display_vtable::acquire_render_lock(gfx);

        super::bitgrid::blit_tiled_raw(
            (*gfx).layer_0,
            sprite_surface,
            dst_x,
            dst_y,
            blit_w,
            blit_h,
            (*gfx).base.clip_x1,
            (*gfx).base.clip_x2,
            orient_local,
        );
        return;
    }

    // Determine color table pointer
    let color_table: *const u8 = if (high_flags & 0x0020_0000) != 0 {
        (*gfx).color_add_table.as_ptr()
    } else if (high_flags & 0x0400_0000) != 0 {
        (*gfx).color_blend_table.as_ptr()
    } else {
        core::ptr::null()
    };

    display_vtable::acquire_render_lock(gfx);

    // src_x=0, src_y=0 always — vtable[33] already set up the sprite surface
    super::bitgrid::blit_impl(
        (*gfx).layer_0,
        dst_x,
        dst_y,
        blit_w,
        blit_h,
        sprite_surface,
        0,
        0,
        color_table,
        orient_local,
    );
}

// =========================================================================
// Bitmap sprite bridges (naked asm for usercall conventions)
// =========================================================================

/// Call DisplayGfx__GetBitmapSpriteInfo (0x573C50).
/// Usercall: EAX=bitmap_obj, EDX=palette, 6 stack params (output ptrs), RET 0x18.
#[unsafe(naked)]
unsafe extern "cdecl" fn wa_get_bitmap_sprite_info(
    _bitmap_obj: *mut u8,
    _palette: u32,
    _out_w: *mut i32,
    _out_h: *mut i32,
    _out_left: *mut i32,
    _out_top: *mut i32,
    _out_right: *mut i32,
    _out_bottom: *mut i32,
    _target: u32,
) -> *const u8 {
    core::arch::naked_asm!(
        "mov eax, [esp + 4]",        // bitmap_obj
        "mov edx, [esp + 8]",        // palette
        "mov ecx, [esp + 36]",       // target
        "push dword ptr [esp + 32]", // out_bottom
        "push dword ptr [esp + 32]", // out_right
        "push dword ptr [esp + 32]", // out_top
        "push dword ptr [esp + 32]", // out_left
        "push dword ptr [esp + 32]", // out_h
        "push dword ptr [esp + 32]", // out_w
        "call ecx",                  // RET 0x18 cleans 6 params
        "ret",
    );
}

/// Call DisplayGfx__BlitBitmapClipped (0x56A700).
/// Usercall: EAX=this, EDX=width, 5 stack params (dst_x, dst_y, height, frame_data, flags), RET 0x14.
#[unsafe(naked)]
unsafe extern "cdecl" fn wa_blit_bitmap_clipped(
    _this: *mut u8,
    _width: u32,
    _dst_x: i32,
    _dst_y: i32,
    _height: i32,
    _frame_data: *const u8,
    _flags: u32,
    _target: u32,
) {
    core::arch::naked_asm!(
        "mov eax, [esp + 4]",        // this
        "mov edx, [esp + 8]",        // width
        "mov ecx, [esp + 32]",       // target
        "push dword ptr [esp + 28]", // flags
        "push dword ptr [esp + 28]", // frame_data
        "push dword ptr [esp + 28]", // height
        "push dword ptr [esp + 28]", // dst_y
        "push dword ptr [esp + 28]", // dst_x
        "call ecx",                  // RET 0x14 cleans 5 params
        "ret",
    );
}

/// Call DisplayGfx__BlitBitmapTiled (0x56A7D0).
/// Usercall: EAX=initial_x, EDI=tile_width, 4 stack params (this, dst_y, height, frame_data), RET 0x10.
#[unsafe(naked)]
unsafe extern "cdecl" fn wa_blit_bitmap_tiled(
    _initial_x: i32,
    _tile_width: i32,
    _this: *mut u8,
    _dst_y: i32,
    _height: i32,
    _frame_data: *const u8,
    _target: u32,
) {
    core::arch::naked_asm!(
        "push edi",
        "mov eax, [esp + 8]",        // initial_x
        "mov edi, [esp + 12]",       // tile_width
        "mov ecx, [esp + 32]",       // target (offset +4 from push edi)
        "push dword ptr [esp + 28]", // frame_data
        "push dword ptr [esp + 28]", // height
        "push dword ptr [esp + 28]", // dst_y
        "push dword ptr [esp + 28]", // this
        "call ecx",                  // RET 0x10 cleans 4 params
        "pop edi",
        "ret",
    );
}

/// Thiscall wrapper for DisplayGfx::DrawScaledSprite (slot 20).
///
/// Computes coordinates in core, then dispatches the blit via blit_impl.
unsafe extern "thiscall" fn draw_scaled_sprite(
    this: *mut DisplayGfx,
    x: Fixed,
    y: Fixed,
    sprite: *mut DisplayBitGrid,
    src_x: i32,
    src_y: i32,
    src_w: i32,
    src_h: i32,
    flags: u32,
) {
    use openwa_core::display::display_vtable::{self, DrawScaledSpriteResult};

    match display_vtable::draw_scaled_sprite(this, x, y, sprite, src_x, src_y, src_w, src_h, flags)
    {
        DrawScaledSpriteResult::Blit {
            layer,
            dst_x,
            dst_y,
            width,
            height,
            sprite,
            src_x,
            src_y,
            color_table,
            blit_flags,
        } => {
            super::bitgrid::blit_impl(
                layer,
                dst_x,
                dst_y,
                width,
                height,
                sprite,
                src_x,
                src_y,
                color_table,
                blit_flags,
            );
        }
        DrawScaledSpriteResult::Stippled {
            layer,
            dst_x,
            dst_y,
            width,
            height,
            sprite,
            src_x,
            src_y,
            stipple_mode,
        } => {
            let parity = *(rb(openwa_core::address::va::G_STIPPLE_PARITY) as *const u32);
            super::bitgrid::blit_stippled_raw(
                layer,
                sprite,
                dst_x,
                dst_y,
                width,
                height,
                src_x,
                src_y,
                stipple_mode,
                parity,
            );
        }
        DrawScaledSpriteResult::Handled => {}
    }
}

pub fn install() -> Result<(), String> {
    let _ = log_line("[Display] Patching DisplayBase vtables");

    unsafe {
        let purecall_addr = rb(PURECALL);
        let noop_addr = noop_thiscall as *const () as u32;

        // Patch primary vtable (0x6645F8): replace _purecall with no-ops.
        let primary = rb(va::DISPLAY_BASE_VTABLE) as *mut u32;
        patch_vtable(primary, VTABLE_SLOTS, |vt| {
            let mut patched = 0u32;
            for i in 0..VTABLE_SLOTS {
                let slot = vt.add(i);
                if *slot == purecall_addr {
                    *slot = noop_addr;
                    patched += 1;
                }
            }
            let _ = log_line(&format!(
                "[Display]   Primary: patched {patched}/{VTABLE_SLOTS} _purecall → no-op"
            ));
        })?;

        // Patch headless vtable (0x66A0F8): replace destructor (slot 0)
        // with our Rust version that frees the Rust-allocated sprite cache.
        let headless = rb(va::DISPLAY_BASE_HEADLESS_VTABLE) as *mut u32;
        patch_vtable(headless, VTABLE_SLOTS, |vt| {
            *vt = headless_destructor as *const () as u32;
            let _ = log_line("[Display]   Headless: patched slot 0 (destructor) → Rust");
        })?;

        // Patch DisplayGfx vtable (0x66A218): replace ported methods with Rust.
        vtable_replace!(DisplayVtable, va::DISPLAY_GFX_VTABLE, {
            get_dimensions      => display_vtable::get_dimensions,
            set_layer_color     => display_vtable::set_layer_color,
            set_active_layer    => display_vtable::set_active_layer,
            get_sprite_info     => display_vtable::get_sprite_info,
            draw_polyline       => display_vtable::draw_polyline,
            draw_line           => display_vtable::draw_line,
            draw_line_clipped   => display_vtable::draw_line_clipped,
            draw_pixel_strip    => display_vtable::draw_pixel_strip,
            draw_crosshair      => display_vtable::draw_crosshair,
            draw_outlined_pixel => display_vtable::draw_outlined_pixel,
            fill_rect           => display_vtable::fill_rect,
            draw_via_callback   => display_vtable::draw_via_callback,
            flush_render        => display_vtable::flush_render,
            set_camera_offset   => display_vtable::set_camera_offset,
            set_clip_rect       => display_vtable::set_clip_rect,
            is_sprite_loaded    => display_vtable::is_sprite_loaded,
            draw_scaled_sprite  => draw_scaled_sprite,
            set_layer_visibility => display_vtable::set_layer_visibility,
            update_palette      => display_vtable::update_palette,
            slot 19 => blit_sprite,
        })?;
        let _ = log_line("[Display]   DisplayGfx: patched 20 methods → Rust");
    }

    Ok(())
}
