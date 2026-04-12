//! DisplayGfx vtable patches — DisplayBase pure-call stubs, headless destructor,
//! and ported DisplayGfx methods.
//!
//! Patches DisplayBase vtables in WA.exe's .rdata:
//! - Primary vtable (0x6645F8): replaces _purecall slots with safe no-op stubs
//! - Headless vtable (0x66A0F8): replaces destructor with Rust version that
//!   correctly frees our Rust-allocated sprite cache sub-objects
//! - DisplayGfx vtable (0x66A218): replaces ported methods with Rust

use core::ffi::c_char;

use openwa_core::address::va;
use openwa_core::bitgrid::DisplayBitGrid;
use openwa_core::fixed::Fixed;
use openwa_core::rebase::rb;
use openwa_core::render::display::destructor as display_destructor;
use openwa_core::render::display::vtable::{self as display_vtable_impl, DisplayGfxVtable};
use openwa_core::render::display::DisplayBase;
use openwa_core::render::display::DisplayGfx;
use openwa_core::render::sprite::gfx_dir::GfxDir;
use openwa_core::vtable::patch_vtable;
use openwa_core::vtable_replace;
use openwa_core::wa_alloc::wa_free;

use crate::hook;
use crate::log_line;

/// The _purecall function address (calls abort).
const PURECALL: u32 = 0x005D_4E16;

/// Number of slots in the DisplayBase vtable.
const VTABLE_SLOTS: usize = 32;

unsafe extern "thiscall" fn noop_thiscall(_this: *mut u8) {}

/// Rust destructor for headless DisplayBase. Frees the sprite cache chain
/// (SpriteCache -> FrameCache -> buffer) that was allocated by new_headless().
unsafe extern "thiscall" fn headless_destructor(
    this: *mut DisplayBase,
    flags: u8,
) -> *mut DisplayBase {
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

// No saved originals needed — all paths are fully ported or use direct bridges.

/// Thiscall stub for DisplayGfx::BlitSprite (slot 19, 0x56B080).
/// Implementation in `openwa_core::render::display::blit_sprite`.
unsafe extern "thiscall" fn blit_sprite(
    this: *mut DisplayGfx,
    x: Fixed,
    y: Fixed,
    sprite_flags: u32,
    palette: u32,
) {
    openwa_core::render::display::blit_sprite::blit_sprite(this, x, y, sprite_flags, palette);
}

/// Thiscall stub for DisplayGfx::DrawScaledSprite (slot 20).
/// Implementation in `openwa_core::render::display::blit_sprite`.
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
    openwa_core::render::display::blit_sprite::draw_scaled_sprite(
        this, x, y, sprite, src_x, src_y, src_w, src_h, flags,
    );
}

// =========================================================================
// Font vtable method wrappers
// =========================================================================
//
// Font slot wrappers (slots 7/8/9/10/34/35/36) all live in openwa-core's
// `display::vtable` module. The wrappers in this file (slots 31/34/35/37
// for sprite/font loading) are thin forwarders only because they need to
// capture a bridge function pointer (`wa_load_sprite_from_vfs` etc.) at
// install time.

// =========================================================================
// Sprite loading vtable method wrappers
// =========================================================================

/// Thiscall wrapper for DisplayGfx::LoadSprite (vtable slot 31, 0x523400).
unsafe extern "thiscall" fn load_sprite(
    this: *mut DisplayGfx,
    layer: u32,
    id: u32,
    flag: u32,
    gfx_dir: *mut GfxDir,
    name: *const c_char,
) -> i32 {
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

/// Bridge to LoadSpriteFromVfs (0x4FAAF0).
/// Usercall: ECX=gfx, EAX=name, stack(sprite, layer_ctx), RET 0x8.
///
/// Verified from caller at 0x523489:
///   ECX ← layer_contexts[layer] (gfx/VFS context)... wait, re-checked:
///   ECX ← gfx param, EAX ← name param.
///   Stack: sprite (EDI from ConstructSprite), layer_ctx.
#[unsafe(naked)]
unsafe extern "cdecl" fn wa_load_sprite_from_vfs(
    _sprite: *mut openwa_core::render::sprite::Sprite,
    _gfx_dir: *mut GfxDir,
    _name: *const c_char,
    _layer_ctx: *mut openwa_core::render::palette::PaletteContext,
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

// GetSpriteFrameForBlit (slot 33) is NOT ported — see the docstring on
// `DisplayVtable::get_sprite_frame_for_blit` in openwa-core for the full
// rationale. Our `blit_sprite` (slot 19) above calls it via the bound
// vtable wrapper on every sprite render.

unsafe extern "thiscall" fn load_sprite_by_layer(
    this: *mut DisplayGfx,
    layer: u32,
    id: u32,
    gfx_dir: *mut GfxDir,
    name: *const c_char,
) -> i32 {
    display_vtable_impl::load_sprite_by_layer(this, layer, id, gfx_dir, name)
}

unsafe extern "thiscall" fn load_font(
    this: *mut DisplayGfx,
    mode: u32,
    font_id: i32,
    gfx_dir: *mut GfxDir,
    filename: *const c_char,
) -> u32 {
    display_vtable_impl::load_font(this, mode, font_id, gfx_dir, filename)
}

unsafe extern "thiscall" fn load_font_extension(
    this: *mut DisplayGfx,
    font_id: i32,
    path: *const c_char,
    char_map: *const c_char,
    palette_value: u32,
    flag: i32,
) -> u32 {
    display_vtable_impl::load_font_extension(this, font_id, path, char_map, palette_value, flag)
}

// ---------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------

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
                "[Display]   Primary: patched {patched}/{VTABLE_SLOTS} _purecall -> no-op"
            ));
        })?;

        // Patch headless vtable (0x66A0F8): replace destructor (slot 0)
        // with our Rust version that frees the Rust-allocated sprite cache.
        let headless = rb(va::DISPLAY_BASE_HEADLESS_VTABLE) as *mut u32;
        patch_vtable(headless, VTABLE_SLOTS, |vt| {
            *vt = headless_destructor as *const () as u32;
            let _ = log_line("[Display]   Headless: patched slot 0 (destructor) -> Rust");
        })?;

        // Initialize bridge address statics for sprite loading
        LOAD_SPRITE_FROM_VFS_ADDR = rb(va::LOAD_SPRITE_FROM_VFS);

        // Patch DisplayGfx vtable (0x66A218): replace ported methods with Rust.
        vtable_replace!(DisplayGfxVtable, va::DISPLAY_GFX_VTABLE, {
            destructor          => display_destructor::display_gfx_destructor,
            get_dimensions      => display_vtable_impl::get_dimensions,
            set_layer_color     => display_vtable_impl::set_layer_color,
            set_active_layer    => display_vtable_impl::set_active_layer,
            get_sprite_info     => display_vtable_impl::get_sprite_info,
            draw_text_on_bitmap => display_vtable_impl::draw_text_on_bitmap,
            draw_tiled_bitmap   => display_vtable_impl::draw_tiled_bitmap,
            get_font_info       => display_vtable_impl::get_font_info,
            get_font_metric     => display_vtable_impl::get_font_metric,
            set_font_param      => display_vtable_impl::set_font_param,
            draw_polyline       => display_vtable_impl::draw_polyline,
            draw_line           => display_vtable_impl::draw_line,
            draw_line_clipped   => display_vtable_impl::draw_line_clipped,
            draw_pixel_strip    => display_vtable_impl::draw_pixel_strip,
            draw_crosshair      => display_vtable_impl::draw_crosshair,
            draw_outlined_pixel => display_vtable_impl::draw_outlined_pixel,
            fill_rect           => display_vtable_impl::fill_rect,
            draw_via_callback   => display_vtable_impl::draw_via_callback,
            draw_tiled_terrain  => display_vtable_impl::draw_tiled_terrain,
            flush_render        => display_vtable_impl::flush_render,
            set_camera_offset   => display_vtable_impl::set_camera_offset,
            set_clip_rect       => display_vtable_impl::set_clip_rect,
            is_sprite_loaded    => display_vtable_impl::is_sprite_loaded,
            load_sprite          => load_sprite,
            draw_scaled_sprite  => draw_scaled_sprite,
            set_layer_visibility => display_vtable_impl::set_layer_visibility,
            update_palette      => display_vtable_impl::update_palette,
            set_font_palette    => display_vtable_impl::set_font_palette,
            slot 19 => blit_sprite,
            get_sprite_frame_for_blit => display_vtable_impl::get_sprite_frame_for_blit,
            load_sprite_by_layer => load_sprite_by_layer,
            load_font            => load_font,
            load_font_extension  => load_font_extension,
        })?;
        let _ = log_line("[Display]   DisplayGfx: patched 32 methods -> Rust");

        // DisplayGfx::LoadSpriteEx (vtable slot 30) has zero callers in both
        // WA.exe (no instructions reach vtable[+0x78] on a DisplayGfx) and our
        // own Rust code (no `bind_DisplayVtable!` invocation, no direct call).
        // Trap it so we'll be alerted the moment any future caller appears.
        hook::install_trap!("DisplayGfx__LoadSpriteEx", va::DISPLAY_GFX_LOAD_SPRITE_EX);

        // ── Slot 33 leaf functions: trap the WA-side originals ────────
        //
        // After `vtable_replace!` swapped slot 33 to our Rust impl, the
        // original `DisplayGfx::GetSpriteFrameForBlit` (0x5237C0) is no
        // longer reached, which means its leaves have no remaining
        // callers in shipping WA either:
        //
        // - `Sprite__GetFrameForBlit` (0x4FAD30): only caller was the
        //   original slot 33 dispatcher.
        // - `SpriteBank__GetFrameForBlit` (0x4F9710): same. Also bank
        //   objects are themselves unreachable (LoadSpriteEx is trapped).
        // - `FrameCache__Allocate` (0x4FA950): only callers were the
        //   two GetFrameForBlit helpers above.
        // - `DisplayBitGrid::SetExternalBuffer` (0x4F6470): only caller
        //   was `SpriteBank__GetFrameForBlit`.
        //
        // `Sprite_LZSS_Decode` (0x5B29E0) is NOT trapped — it has
        // independent callers (`IMG_Decode`, `FUN_0048EE32`) reachable
        // from `DDGame__Constructor`/`SoundEmitter`/`LoadHudAndWeaponSprites`,
        // all still bridged. Trapping it would break those.
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

        // ── Slot 0 destructor leaves: trap the WA-side originals ──────
        //
        // After `vtable_replace!` swapped slot 0 to our Rust impl, the
        // original `DisplayGfx::DestructorImpl` (0x56A010) is no longer
        // reached, so its only-callee helpers lose their last live
        // caller too.
        //
        // `TileBitmapSet::Destructor` (0x569BC0) has a *second* xref to
        // `FUN_0056C2E0` (a tile-bitmap-set realloc helper called only
        // from `CBitmap__Constructor_Maybe` at 0x56C060). That second
        // chain is **structurally dead** in shipping WA: 0x56C060 sits
        // in the DisplayGfx vtable .rdata block at slot index 38 (offset
        // 0x98 past the vtable base 0x66A218), but no instruction in
        // WA.exe loads from offset 0x98 of any pointer — verified by
        // byte-pattern search for `FF 90 98 00 00 00` (CALL [reg+0x98])
        // and `8B 4? 98` (MOV reg,[reg+0x98]) returning no DisplayGfx
        // hits. The compiler emitted slots 38/39 of DisplayGfx into
        // .rdata but the C++ source has no live caller for them — they
        // are vestigial vtable entries from a removed feature. So
        // trapping `TileBitmapSet::Destructor` is safe.
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
