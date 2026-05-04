//! DisplayGfx vtable patches — DisplayBase purecall stubs, headless destructor,
//! and ported DisplayGfx method wiring.

use core::ffi::c_char;

use openwa_core::fixed::Fixed;
use openwa_game::address::va;
use openwa_game::asset::gfx_dir::GfxDir;
use openwa_game::bitgrid::DisplayBitGrid;
use openwa_game::rebase::rb;
use openwa_game::render::SpriteOp;
use openwa_game::render::display::DisplayBase;
use openwa_game::render::display::DisplayGfx;
use openwa_game::render::display::destructor as display_destructor;
use openwa_game::render::display::vtable::{self as display_vtable_impl, DisplayGfxVtable};
use openwa_game::render::palette::PaletteContext;
use openwa_game::render::sprite::Sprite;
use openwa_game::vtable::patch_vtable;
use openwa_game::vtable_replace;
use openwa_game::wa_alloc::wa_free;

use crate::hook;

/// The _purecall function address (calls abort).
const PURECALL: u32 = 0x005D4E16;

/// Number of slots in the DisplayBase vtable.
const VTABLE_SLOTS: usize = 32;

unsafe extern "thiscall" fn noop_thiscall(_this: *mut u8) {}

unsafe extern "thiscall" fn headless_destructor(
    this: *mut DisplayBase,
    flags: u8,
) -> *mut DisplayBase {
    unsafe {
        let sprite_cache = (*this).sprite_cache;
        if !sprite_cache.is_null() {
            let frame_cache = (*sprite_cache).frame_cache;
            if !frame_cache.is_null() {
                let buf = (*frame_cache).buffer;
                if !buf.is_null() {
                    wa_free(buf);
                }
                wa_free(frame_cache);
            }
            wa_free(sprite_cache);
        }
        if flags & 1 != 0 {
            wa_free(this);
        }
        this
    }
}

// BlitSprite (slot 19, 0x56B080)
unsafe extern "thiscall" fn blit_sprite(
    this: *mut DisplayGfx,
    x: Fixed,
    y: Fixed,
    sprite: SpriteOp,
    palette: u32,
) {
    unsafe {
        openwa_game::render::display::blit_sprite::blit_sprite(this, x, y, sprite, palette);
    }
}

// DrawScaledSprite (slot 20, 0x56B5F0)
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
    unsafe {
        openwa_game::render::display::blit_sprite::draw_scaled_sprite(
            this, x, y, sprite, src_x, src_y, src_w, src_h, flags,
        );
    }
}

// LoadSprite (slot 31, 0x523400)
unsafe extern "thiscall" fn load_sprite(
    this: *mut DisplayGfx,
    layer: u32,
    id: u32,
    flag: u32,
    gfx_dir: *mut GfxDir,
    name: *const c_char,
) -> i32 {
    unsafe {
        display_vtable_impl::load_sprite(
            this,
            layer,
            id,
            flag,
            gfx_dir,
            name,
            wa_load_sprite_from_vfs,
        )
    }
}

// LoadSpriteFromVfs (0x4FAAF0) — naked bridge
#[unsafe(naked)]
unsafe extern "cdecl" fn wa_load_sprite_from_vfs(
    _sprite: *mut Sprite,
    _gfx_dir: *mut GfxDir,
    _name: *const c_char,
    _layer_ctx: *mut PaletteContext,
) -> i32 {
    core::arch::naked_asm!(
        // cdecl: +4=sprite, +8=gfx, +12=name, +16=layer_ctx
        "mov ecx, [esp+8]",         // gfx → ECX
        "mov eax, [esp+12]",        // name → EAX
        "push dword ptr [esp+16]",  // layer_ctx
        "push dword ptr [esp+8]",   // sprite (shifted +4 by push)
        "call [{ADDR}]",            // RET 0x8 cleans 2 stack params
        "ret",
        ADDR = sym LOAD_SPRITE_FROM_VFS_ADDR,
    );
}

static mut LOAD_SPRITE_FROM_VFS_ADDR: u32 = 0;

// LoadSpriteByLayer (slot 34)
unsafe extern "thiscall" fn load_sprite_by_layer(
    this: *mut DisplayGfx,
    layer: u32,
    id: u32,
    gfx_dir: *mut GfxDir,
    name: *const c_char,
) -> i32 {
    unsafe { display_vtable_impl::load_sprite_by_layer(this, layer, id, gfx_dir, name) }
}

// LoadFont (slot 35)
unsafe extern "thiscall" fn load_font(
    this: *mut DisplayGfx,
    mode: u32,
    font_id: i32,
    gfx_dir: *mut GfxDir,
    filename: *const c_char,
) -> u32 {
    unsafe { display_vtable_impl::load_font(this, mode, font_id, gfx_dir, filename) }
}

// LoadFontExtension (slot 36)
unsafe extern "thiscall" fn load_font_extension(
    this: *mut DisplayGfx,
    font_id: i32,
    path: *const c_char,
    char_map: *const c_char,
    palette_value: u32,
    flag: i32,
) -> u32 {
    unsafe {
        display_vtable_impl::load_font_extension(this, font_id, path, char_map, palette_value, flag)
    }
}

pub fn install() -> Result<(), String> {
    unsafe {
        let purecall_addr = rb(PURECALL);
        let noop_addr = noop_thiscall as *const () as u32;

        let primary = rb(va::DISPLAY_BASE_VTABLE) as *mut u32;
        patch_vtable(primary, VTABLE_SLOTS, |vt| {
            for i in 0..VTABLE_SLOTS {
                let slot = vt.add(i);
                if *slot == purecall_addr {
                    *slot = noop_addr;
                }
            }
        })?;

        let headless = rb(va::DISPLAY_BASE_HEADLESS_VTABLE) as *mut u32;
        patch_vtable(headless, VTABLE_SLOTS, |vt| {
            *vt = headless_destructor as *const () as u32;
        })?;

        LOAD_SPRITE_FROM_VFS_ADDR = rb(va::LOAD_SPRITE_FROM_VFS);

        vtable_replace!(DisplayGfxVtable, va::DISPLAY_GFX_VTABLE, {
            destructor                 => display_destructor::display_gfx_destructor,
            get_dimensions             => display_vtable_impl::get_dimensions,
            set_layer_color            => display_vtable_impl::set_layer_color,
            set_active_layer           => display_vtable_impl::set_active_layer,
            get_sprite_info            => display_vtable_impl::get_sprite_info,
            draw_text_on_bitmap        => display_vtable_impl::draw_text_on_bitmap,
            draw_tiled_bitmap          => display_vtable_impl::draw_tiled_bitmap,
            get_font_info              => display_vtable_impl::get_font_info,
            get_font_metric            => display_vtable_impl::get_font_metric,
            measure_text               => display_vtable_impl::measure_text_bridge,
            draw_polyline              => display_vtable_impl::draw_polyline,
            draw_line                  => display_vtable_impl::draw_line,
            draw_line_clipped          => display_vtable_impl::draw_line_clipped,
            draw_pixel_strip           => display_vtable_impl::draw_pixel_strip,
            draw_crosshair             => display_vtable_impl::draw_crosshair,
            draw_outlined_pixel        => display_vtable_impl::draw_outlined_pixel,
            fill_rect                  => display_vtable_impl::fill_rect,
            draw_via_callback          => display_vtable_impl::draw_via_callback,
            draw_tiled_terrain         => display_vtable_impl::draw_tiled_terrain,
            flush_render               => display_vtable_impl::flush_render,
            set_camera_offset          => display_vtable_impl::set_camera_offset,
            set_clip_rect              => display_vtable_impl::set_clip_rect,
            is_sprite_loaded           => display_vtable_impl::is_sprite_loaded,
            load_sprite                => load_sprite,
            draw_scaled_sprite         => draw_scaled_sprite,
            set_layer_visibility       => display_vtable_impl::set_layer_visibility,
            update_palette             => display_vtable_impl::update_palette,
            set_font_palette           => display_vtable_impl::set_font_palette,
            blit_sprite                => blit_sprite,
            get_sprite_frame_for_blit  => display_vtable_impl::get_sprite_frame_for_blit,
            load_sprite_by_layer       => load_sprite_by_layer,
            load_font                  => load_font,
            load_font_extension        => load_font_extension,
        })?;

        // Slot 30: zero callers in WA.exe or Rust.
        hook::install_trap!("DisplayGfx__LoadSpriteEx", va::DISPLAY_GFX_LOAD_SPRITE_EX);

        // Slot 33 leaf functions — only callers were the original WA-side
        // GetSpriteFrameForBlit (now replaced). Note: Sprite_LZSS_Decode
        // (0x5B29E0) is NOT trapped — it has independent live callers.
        hook::install_trap!("Sprite__GetFrameForBlit", va::SPRITE_GET_FRAME_FOR_BLIT);
        hook::install_trap!(
            "SpriteBank__GetFrameForBlit",
            va::SPRITE_BANK_GET_FRAME_FOR_BLIT
        );
        hook::install_trap!("FrameCache__Allocate", va::FRAME_CACHE_ALLOCATE);
        hook::install_trap!(
            "DisplayBitGrid__SetExternalBuffer",
            va::DISPLAY_BIT_GRID_SET_EXTERNAL_BUFFER
        );

        // Slot 0 destructor leaf functions — only caller was the original
        // WA-side DestructorImpl (now replaced). TileBitmapSet::Destructor's
        // second xref (via slot 38) is structurally dead — no instruction
        // in WA.exe dispatches through vtable offset 0x98.
        hook::install_trap!(
            "DisplayGfx__DestructorImpl",
            va::DISPLAY_GFX_DESTRUCTOR_IMPL
        );
        hook::install_trap!(
            "DisplayGfx__FreeLayerSpriteTable",
            va::DISPLAY_GFX_FREE_LAYER_SPRITE_TABLE
        );
        hook::install_trap!("TileBitmapSet__Destructor", va::TILE_BITMAP_SET_DESTRUCTOR);
    }

    Ok(())
}
