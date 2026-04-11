use core::ffi::c_char;

use openwa_core::vtable;

use crate::fixed::Fixed;
use crate::render::display::font::{
    font_extend, font_get_info_impl, font_get_metric_impl, font_load_from_gfx,
    font_set_palette_impl, font_set_param_impl, Font,
};
use crate::render::display::layer::Layer;
use crate::render::display::line_draw::Vertex;
use crate::render::sprite::gfx_dir::GfxDir;
use crate::render::sprite::sprite::{LayerSprite, LayerSpriteFrame};
use crate::render::SpriteCache;
use crate::wa_alloc::wa_malloc_struct_zeroed;

/// DisplayVtable — vtable for the display/rendering subsystem (DisplayGfx).
///
/// Constructor: DisplayGfx__Init (0x569D00).
/// Vtable: 0x66A218 (38 slots).
/// Destructor: 0x569CE0.
///
/// DisplayGfx (0x24E28 bytes) extends DisplayBase.
/// Manages layers, sprites, fonts, palettes, and delegates rendering through
/// RenderContext (g_RenderContext at 0x79D6D4), which in turn dispatches
/// to a renderer backend (CompatRenderer for D3D/DDraw, OpenGLCPU for OpenGL).
///
/// ## Dispatch chain
///
/// ```text
/// DisplayGfx  →  RenderContext         →  RendererBackend
///   vtable        (CWormsApp sub-          (CompatRenderer/
///   0x66A218       object, vtable           OpenGLCPU/DDraw)
///                  0x662EC8)
/// ```
///
/// DisplayGfx methods apply camera offset and clipping, then call through
/// `g_RenderContext->vtable[N]` for the actual rendering operation.

/// Display vtable (0x66A218, 38 slots).
///
/// Slots 2, 3, 25, 29 are stubs (CGameTask no-ops).
/// All other slots are standard thiscall.
///
/// Coordinate conventions:
/// - Methods that add `camera * 0x10000` take Fixed (16.16) coordinates.
/// - Methods that add `camera` directly take pixel-integer coordinates.
#[vtable(size = 38, va = 0x0066_A218, class = "DisplayGfx")]
pub struct DisplayGfxVtable {
    /// scalar deleting destructor (original at 0x569CE0, RET 0x4).
    ///
    /// **Ported.** Native Rust impl is
    /// [`crate::render::display::destructor::display_gfx_destructor`].
    /// The thunk runs the cleanup body (`DestructorImpl`, originally
    /// 0x56A010) and then `_free(this)` if `flags & 1` is set, returning
    /// `this` per the MSVC ABI. Wired via `vtable_replace!` in
    /// `install_display`. The three WA-side helpers
    /// (`DestructorImpl`, `FreeLayerSpriteTable`,
    /// `TileBitmapSet::Destructor`) are trapped — see commit 5 of the
    /// destructor port for the dead-slot analysis behind trapping the
    /// `TileBitmapSet` helper.
    #[slot(0)]
    pub destructor: fn(this: *mut DisplayGfx, flags: u32) -> *mut DisplayGfx,
    /// get display dimensions in pixels (0x56A460, RET 0x8)
    #[slot(1)]
    pub get_dimensions: fn(this: *mut DisplayGfx, out_w: *mut u32, out_h: *mut u32),
    /// set layer color (0x5231E0, RET 0x8)
    #[slot(4)]
    pub set_layer_color: fn(this: *mut DisplayGfx, layer: i32, color: i32),
    /// set active layer, returns layer context ptr (0x523270, RET 0x4).
    ///
    /// Returns the `PaletteContext*` for the requested layer (1-3),
    /// or null if the index is out of range. Callers feed the result to
    /// `update_palette` (slot 24) and to GfxDir loaders that need a
    /// palette context for color remapping.
    #[slot(5)]
    pub set_active_layer: fn(this: *mut DisplayGfx, layer: i32) -> *mut PaletteContext,
    /// get sprite info by layer and id (0x523500, RET 0x10)
    ///
    /// Looks up sprite metadata for the given layer ID. Checks sprite_ptrs
    /// (Sprite* pointers) first, then sprite_banks. On success, writes
    /// sprite data, flags, and width to the output pointers.
    /// Returns a pointer to the string "sprite" on success, or 0 on failure.
    #[slot(6)]
    pub get_sprite_info: fn(
        this: *mut DisplayGfx,
        layer: i32,
        out_data: *mut u32,
        out_flags: *mut u32,
        out_width: *mut u32,
    ) -> u32,
    /// draw bitmap text onto a `BitGrid` surface (0x5236B0, RET 0x1C).
    ///
    /// `font_id` low 16 bits = font slot (1-based, validated against
    /// `font_table[1..=31]`); `font_id` high 16 bits = flags. Bit 1 of the
    /// high half flips the rasterizer to right-aligned mode (draws the
    /// string right-to-left starting from `pen_x` as the right anchor).
    ///
    /// `bitmap` is the destination `BitGrid` (caller passes a layer surface
    /// or sprite-bitmap pointer). `pen_x`/`pen_y` are the top-left corner
    /// of the text area (or right edge in right-aligned mode).
    ///
    /// `out_pen_x` receives the running advance after drawing (the X-pixel
    /// distance covered by the rendered text). `out_width` always receives
    /// the font's max-glyph-width unconditionally.
    ///
    /// Returns the count of characters successfully drawn (or the index of
    /// the first character that didn't fit if truncated, or `-1` for the
    /// right-aligned path completing the full string, or 0 on early
    /// validation failure).
    #[slot(7)]
    pub draw_text_on_bitmap: fn(
        this: *mut DisplayGfx,
        font_id: i32,
        bitmap: *mut BitGrid,
        pen_x: i32,
        pen_y: i32,
        msg: *const c_char,
        out_pen_x: *mut i32,
        out_width: *mut i32,
    ) -> i32,
    /// get font info for a font slot (0x523790, RET 0xC)
    ///
    /// Reads two signed shorts from the font object and writes them to output pointers.
    /// Returns 1 on success, 0 if font_id is out of range or font slot is empty.
    #[slot(8)]
    pub get_font_info:
        fn(this: *mut DisplayGfx, font_id: i32, out_1: *mut u32, out_2: *mut u32) -> u32,
    /// get font metric for a character (0x523750, RET 0x10)
    ///
    /// Queries character metrics from the font object. `char_code` is passed
    /// as a byte (low 8 bits). Returns result via two output pointers.
    /// Returns 1 on success, 0 if font_id is out of range or font slot is empty.
    #[slot(9)]
    pub get_font_metric: fn(
        this: *mut DisplayGfx,
        font_id: i32,
        char_code: u32,
        out_1: *mut u32,
        out_2: *mut u32,
    ) -> u32,
    /// set font rendering parameter (0x523710, RET 0x10)
    #[slot(10)]
    pub set_font_param: fn(this: *mut DisplayGfx, font_id: i32, p3: u32, p4: u32, p5: u32) -> u32,
    /// `DisplayGfx::DrawTiledBitmap` (0x56B8C0, RET 0xC).
    ///
    /// Three-phase tile-cached landscape blit (allocate / populate /
    /// display). `source` is a [`TiledBitmapSource`] descriptor. Reachable
    /// at runtime via `RenderDrawingQueue` case 0xD; only known producer is
    /// `CTaskLand::RenderLandscape`. Ported impl at [`draw_tiled_bitmap_impl`].
    #[slot(11)]
    pub draw_tiled_bitmap:
        fn(this: *mut DisplayGfx, dest_x: i32, dest_y: i32, source: *const TiledBitmapSource),
    /// draw polyline with camera offset (0x56BCC0, RET 0xC)
    ///
    /// Transforms point array by camera offset, then draws connected line segments.
    /// Points are pixel-integer pairs.
    #[slot(12)]
    pub draw_polyline: fn(this: *mut DisplayGfx, points: *mut i32, count: i32, color: u32),
    /// draw line with camera offset (0x56BDB0, RET 0x18)
    ///
    /// Coordinates are fixed-point; camera is applied as `camera * 0x10000 + coord`.
    #[slot(13)]
    pub draw_line: fn(
        this: *mut DisplayGfx,
        x1: Fixed,
        y1: Fixed,
        x2: Fixed,
        y2: Fixed,
        color1: u32,
        color2: u32,
    ),
    /// draw line with camera offset and clip (0x56BD50, RET 0x14)
    #[slot(14)]
    pub draw_line_clipped:
        fn(this: *mut DisplayGfx, x1: Fixed, y1: Fixed, x2: Fixed, y2: Fixed, color: u32),
    /// draw repeated pixels along a vector (0x56BE10, RET 0x18)
    ///
    /// Draws count+1 pixels starting at (x,y), stepping by (dx,dy) each iteration.
    /// All coordinates are fixed-point.
    #[slot(15)]
    pub draw_pixel_strip:
        fn(this: *mut DisplayGfx, x: Fixed, y: Fixed, dx: Fixed, dy: Fixed, count: i32, color: u32),
    /// draw crosshair pattern — 9 pixels in a cross (0x56BE80, RET 0x10)
    ///
    /// Pixel-integer coordinates (adds camera directly).
    #[slot(16)]
    pub draw_crosshair: fn(this: *mut DisplayGfx, x: i32, y: i32, color_fg: u32, color_bg: u32),
    /// draw outlined pixel — center + 4 cardinal neighbors (0x56BFD0, RET 0x10)
    ///
    /// Pixel-integer coordinates.
    #[slot(17)]
    pub draw_outlined_pixel:
        fn(this: *mut DisplayGfx, x: i32, y: i32, color_fg: u32, color_bg: i32),
    /// fill rectangle with camera offset and clipping (0x56B810, RET 0x14)
    ///
    /// Pixel-integer coordinates (adds camera directly, then clips).
    #[slot(18)]
    pub fill_rect: fn(this: *mut DisplayGfx, x1: i32, y1: i32, x2: i32, y2: i32, color: u32),
    /// blit sprite to display (0x56B080, RET 0x10)
    ///
    /// Complex sprite blitting with orientation, palette, and blend flags.
    /// x/y are fixed-point world coordinates (`>> 0x10` internally).
    /// param_4 low 16 bits = sprite ID, high bits = orientation/blend flags.
    /// param_5 = palette/opacity value.
    ///
    /// Note: the original code also reads EBX, ESI, EDI set by the caller
    /// (sprite width, sprite height, extra flags) via callee-saved registers.
    /// These are not part of the thiscall ABI and cannot be expressed here.
    #[slot(19)]
    pub blit_sprite: fn(this: *mut DisplayGfx, x: Fixed, y: Fixed, sprite_flags: u32, palette: u32),
    /// draw scaled/rotated sprite (0x56B660, RET 0x20)
    ///
    /// x/y are fixed-point world coordinates.
    /// `sprite` is a source DisplayBitGrid pointer.
    /// Dispatches by flags:
    /// - bit 20: blend mode toggle (0 = ColorTable/transparency, 1 = Copy/opaque)
    /// - bit 21 (0x200000): additive blend (uses color_add_table LUT)
    /// - bit 26 (0x4000000): color blend (uses color_blend_table LUT)
    /// - bit 27 (0x8000000): stippled mode 0
    /// - bit 28 (0x10000000): stippled mode 1
    #[slot(20)]
    pub draw_scaled_sprite: fn(
        this: *mut DisplayGfx,
        x: Fixed,
        y: Fixed,
        sprite: *mut DisplayBitGrid,
        src_x: i32,
        src_y: i32,
        src_w: i32,
        src_h: i32,
        flags: u32,
    ),
    /// draw via object callback (0x56B7C0, RET 0x14)
    ///
    /// Calls vtable[2] on the object pointer with camera-adjusted coordinates.
    /// x/y are fixed-point.
    #[slot(21)]
    pub draw_via_callback:
        fn(this: *mut DisplayGfx, x: Fixed, y: Fixed, obj: *mut u8, p5: u32, p6: u32),
    /// draw tiled terrain (0x56C5A0, RET 0x10)
    ///
    /// Tiles bitmaps from a grid config in a row-major pattern with camera offset.
    /// `count` limits the number of pixel-rows rendered. `flags` low 16 bits
    /// selects mode (only 1 supported), bit 19 controls a blit transparency flag.
    #[slot(22)]
    pub draw_tiled_terrain: fn(this: *mut DisplayGfx, x: Fixed, y: Fixed, count: i32, flags: u32),
    /// set layer visibility (0x56A5D0, RET 0x8)
    #[slot(23)]
    pub set_layer_visibility: fn(this: *mut DisplayGfx, layer: i32, visible: i32),
    /// update palette from PaletteContext (0x56A610, RET 0x8)
    #[slot(24)]
    pub update_palette: fn(this: *mut DisplayGfx, palette_ctx: *mut PaletteContext, commit: i32),
    // Slot 25: stub (CGameTask__vt19)
    /// flush pending render state (0x56A580, plain RET)
    ///
    /// No stack params. Releases renderer lock and
    /// calls through RenderContext vtable to finalize.
    #[slot(26)]
    pub flush_render: fn(this: *mut DisplayGfx),
    /// set camera offset (0x56CC40, RET 0x8)
    ///
    /// Fixed-point input; internally `>> 16` to pixel integers stored
    /// in `DisplayGfx::camera_x` / `camera_y`.
    #[slot(27)]
    pub set_camera_offset: fn(this: *mut DisplayGfx, x: Fixed, y: Fixed),
    /// set clip rectangle (0x56CC60, RET 0x10)
    ///
    /// Fixed-point input; internally `>> 16` to pixel integers, clamped to display dimensions.
    #[slot(28)]
    pub set_clip_rect: fn(this: *mut DisplayGfx, x1: Fixed, y1: Fixed, x2: Fixed, y2: Fixed),
    // Slot 29: stub (CGameTask__vt18)
    /// load sprite with extended params (0x523310, RET 0x18)
    ///
    /// Allocates 0x17C sprite object, calls DisplayGfx constructor.
    #[slot(30)]
    pub load_sprite_ex:
        fn(this: *mut DisplayGfx, mode: i32, id: i32, p4: u32, count: i32, p6: u32, p7: u32) -> i32,
    /// load sprite into layer (0x523400, RET 0x14)
    #[slot(31)]
    pub load_sprite: fn(
        this: *mut DisplayGfx,
        layer: u32,
        id: u32,
        flag: u32,
        gfx_dir: *mut GfxDir,
        name: *const c_char,
    ) -> i32,
    /// check if a sprite ID is loaded (0x56A480, RET 0x4)
    ///
    /// Returns 1 if the sprite exists in any of the three sprite arrays.
    #[slot(32)]
    pub is_sprite_loaded: fn(this: *mut DisplayGfx, id: i32) -> u32,
    /// look up a sprite frame's surface and metadata for blitting
    /// (0x5237C0, RET 0x24) — `DisplayGfx::GetSpriteFrameForBlit`.
    #[slot(33)]
    pub get_sprite_frame_for_blit: fn(
        this: *mut DisplayGfx,
        sprite_id: u32,
        anim_value: u32,
        out_w: *mut i32,
        out_h: *mut i32,
        out_left: *mut i32,
        out_top: *mut i32,
        out_right: *mut i32,
        out_bottom: *mut i32,
        out_anim_frac: *mut u32,
    ) -> *mut DisplayBitGrid,
    /// load .fnt bitmap font into a font slot (0x523560, RET 0x10)
    #[slot(34)]
    pub load_font: fn(
        this: *mut DisplayGfx,
        mode: i32,
        font_id: i32,
        gfx_dir: *mut GfxDir,
        filename: *const c_char,
    ) -> u32,
    /// load .fex font extension for a font slot (0x523620, RET 0x14)
    #[slot(35)]
    pub load_font_extension: fn(
        this: *mut DisplayGfx,
        font_id: i32,
        path: *const c_char,
        char_map: *const c_char,
        palette_value: u32,
        flag: i32,
    ) -> u32,
    /// set font palette for all loaded fonts (0x523690, RET 0x8)
    #[slot(36)]
    pub set_font_palette: fn(this: *mut DisplayGfx, font_count: u32, palette_value: u32),
    /// load sprite by layer with fallback allocation (0x56A4C0, RET 0x10)
    #[slot(37)]
    pub load_sprite_by_layer: fn(
        this: *mut DisplayGfx,
        layer: u32,
        id: u32,
        gfx: *mut GfxDir,
        name: *const c_char,
    ) -> i32,
}

// Generate calling wrappers: DisplayGfx::set_layer_color(), etc.
bind_DisplayGfxVtable!(DisplayGfx, base.vtable);

// =========================================================================
// Ported DisplayGfx vtable methods
// =========================================================================

use super::base::DisplayBase;
use super::gfx::DisplayGfx;
use super::line_draw;
use crate::bitgrid::{BitGrid, DisplayBitGrid};
use crate::render::palette::PaletteContext;
use crate::render::sprite::{
    frame_cache::frame_cache_allocate, lzss::sprite_lzss_decode, Sprite, SpriteBank, SpriteVtable,
};

/// Port of DisplayGfx::GetDimensions (slot 1, 0x56A460).
pub unsafe extern "thiscall" fn get_dimensions(
    this: *mut DisplayGfx,
    out_w: *mut u32,
    out_h: *mut u32,
) {
    *out_w = (*this).base.display_width;
    *out_h = (*this).base.display_height;
}

use super::context::{FastcallResult, RenderContext};
use crate::address::va;
use crate::rebase::rb;

/// Port of DisplayGfx::FlushRender (slot 26, 0x56A580).
///
/// The original also calls `unlock_surface_write` when the lock is held,
/// but that's a no-op whose result is discarded — we just clear the flag.
pub unsafe extern "thiscall" fn flush_render(this: *mut DisplayGfx) {
    let wrapper = *(rb(va::G_RENDER_CONTEXT) as *const *mut RenderContext);

    if (*this).render_lock != 0 {
        (*this).render_lock = 0;
    }

    let mut buf = FastcallResult::default();
    RenderContext::get_renderer_surface_raw(wrapper, &mut buf);
}

/// Port of DisplayGfx::SetCameraOffset (slot 27, 0x56CC40).
pub unsafe extern "thiscall" fn set_camera_offset(this: *mut DisplayGfx, x: Fixed, y: Fixed) {
    (*this).camera_x = x.to_int();
    (*this).camera_y = y.to_int();
}

/// Port of DisplayGfx::SetClipRect (slot 28, 0x56CC60).
pub unsafe extern "thiscall" fn set_clip_rect(
    this: *mut DisplayGfx,
    x1: Fixed,
    y1: Fixed,
    x2: Fixed,
    y2: Fixed,
) {
    let base = &mut (*this).base;

    // Convert Fixed to pixel integers
    let mut cx1 = x1.to_int();
    let mut cy1 = y1.to_int();
    let mut cx2 = x2.to_int();
    let mut cy2 = y2.to_int();

    // Store and clamp to display dimensions
    base.clip_x1 = cx1;
    base.clip_y1 = cy1;
    base.clip_x2 = cx2;
    base.clip_y2 = cy2;

    if cx1 < 0 {
        base.clip_x1 = 0;
        cx1 = 0;
    }
    if cy1 < 0 {
        base.clip_y1 = 0;
        cy1 = 0;
    }
    if cx2 > base.display_width as i32 {
        base.clip_x2 = base.display_width as i32;
        cx2 = base.display_width as i32;
    }
    if cy2 > base.display_height as i32 {
        base.clip_y2 = base.display_height as i32;
        cy2 = base.display_height as i32;
    }

    // Mirror clip rect to the layer_0 BitGrid.
    let layer = (*this).layer_0;
    (*layer).clip_left = cx1 as u32;
    (*layer).clip_top = cy1 as u32;
    (*layer).clip_right = cx2 as u32;
    (*layer).clip_bottom = cy2 as u32;

    if cx1 < 0 {
        (*layer).clip_left = 0;
    }
    if cy1 < 0 {
        (*layer).clip_top = 0;
    }
    if cx2 > (*layer).width as i32 {
        (*layer).clip_right = (*layer).width;
    }
    if cy2 > (*layer).height as i32 {
        (*layer).clip_bottom = (*layer).height;
    }
}

/// Port of the render-lock flush helper at 0x56A330 (usercall on ESI).
unsafe fn flush_render_lock(gfx: *mut DisplayGfx) {
    if (*gfx).render_lock != 0 {
        let wrapper = *(rb(va::G_RENDER_CONTEXT) as *const *mut RenderContext);
        let data = (*(*gfx).layer_0).data;
        let mut buf = FastcallResult::default();
        RenderContext::unlock_surface_write_raw(wrapper, &mut buf, data);
        (*gfx).render_lock = 0;
    }
}

/// Port of the render-lock acquire helper at 0x56A370 (usercall on ESI).
pub unsafe fn acquire_render_lock(gfx: *mut DisplayGfx) {
    if (*gfx).render_lock != 0 {
        return; // already locked
    }

    let wrapper = *(rb(va::G_RENDER_CONTEXT) as *const *mut RenderContext);
    let mut buf = FastcallResult::default();

    let mut dims: [u32; 2] = [0; 2];
    RenderContext::get_framebuffer_dims_raw(wrapper, &mut buf, dims.as_mut_ptr());
    let fb_width = dims[0];
    let fb_height = dims[1];

    let mut data_ptr: *mut u8 = core::ptr::null_mut();
    let mut stride: u32 = 0;
    RenderContext::lock_surface_write_raw(wrapper, &mut buf, &mut data_ptr, &mut stride);

    let layer = (*gfx).layer_0;
    if (*layer).external_buffer != 0 {
        (*layer).width = fb_width;
        (*layer).height = fb_height;
        (*layer).data = data_ptr;
        (*layer).row_stride = stride;
        (*layer).clip_left = 0;
        (*layer).clip_top = 0;
        (*layer).clip_right = fb_width;
        (*layer).clip_bottom = fb_height;
    }

    let base = &(*gfx).base;
    (*layer).clip_left = base.clip_x1 as u32;
    (*layer).clip_top = base.clip_y1 as u32;
    (*layer).clip_right = base.clip_x2 as u32;
    (*layer).clip_bottom = base.clip_y2 as u32;

    if base.clip_x1 < 0 {
        (*layer).clip_left = 0;
    }
    if base.clip_y1 < 0 {
        (*layer).clip_top = 0;
    }
    if base.clip_x2 > (*layer).width as i32 {
        (*layer).clip_right = (*layer).width;
    }
    if base.clip_y2 > (*layer).height as i32 {
        (*layer).clip_bottom = (*layer).height;
    }

    (*gfx).render_lock = 1;
}

/// Port of DisplayGfx::FillRect (slot 18, 0x56B810).
pub unsafe extern "thiscall" fn fill_rect(
    this: *mut DisplayGfx,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    color: u32,
) {
    let base = &(*this).base;

    let mut left = x1 + (*this).camera_x;
    let mut top = y1 + (*this).camera_y;
    let mut right = x2 + (*this).camera_x;
    let mut bottom = y2 + (*this).camera_y;

    if right <= base.clip_x1
        || bottom <= base.clip_y1
        || left >= base.clip_x2
        || top >= base.clip_y2
    {
        return;
    }

    if left < base.clip_x1 {
        left = base.clip_x1;
    }
    if top < base.clip_y1 {
        top = base.clip_y1;
    }
    if right > base.clip_x2 {
        right = base.clip_x2;
    }
    if bottom > base.clip_y2 {
        bottom = base.clip_y2;
    }

    flush_render_lock(this);

    let wrapper = *(rb(va::G_RENDER_CONTEXT) as *const *mut RenderContext);
    let mut buf = FastcallResult::default();
    RenderContext::fill_rect_raw(
        wrapper,
        &mut buf,
        left,
        top,
        right - left,
        bottom - top,
        color,
    );
}

/// Port of DisplayGfx::DrawOutlinedPixel (slot 17, 0x56BFD0).
///
/// Center pixel in `color_fg` with 4 cardinal neighbors in `color_bg`
/// (if non-zero).
pub unsafe extern "thiscall" fn draw_outlined_pixel(
    this: *mut DisplayGfx,
    x: i32,
    y: i32,
    color_fg: u32,
    color_bg: i32,
) {
    let cx = x + (*this).camera_x;
    let cy = y + (*this).camera_y;

    acquire_render_lock(this);

    let layer = (*this).layer_0;
    if color_bg != 0 {
        let bg = color_bg as u8;
        DisplayBitGrid::put_pixel_clipped_raw(layer, cx - 1, cy, bg);
        DisplayBitGrid::put_pixel_clipped_raw(layer, cx + 1, cy, bg);
        DisplayBitGrid::put_pixel_clipped_raw(layer, cx, cy - 1, bg);
        DisplayBitGrid::put_pixel_clipped_raw(layer, cx, cy + 1, bg);
    }
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx, cy, color_fg as u8);
}

/// Port of DisplayGfx::DrawCrosshair (slot 16, 0x56BE80).
///
/// 2×2 foreground block with an 8-pixel `color_bg` outline:
///
/// ```text
///     bg bg
///  bg FG FG bg
///  bg FG FG bg
///     bg bg
/// ```
pub unsafe extern "thiscall" fn draw_crosshair(
    this: *mut DisplayGfx,
    x: i32,
    y: i32,
    color_fg: u32,
    color_bg: u32,
) {
    let cx = x + (*this).camera_x;
    let cy = y + (*this).camera_y;

    acquire_render_lock(this);

    let layer = (*this).layer_0;
    let bg = color_bg as u8;
    let fg = color_fg as u8;

    DisplayBitGrid::put_pixel_clipped_raw(layer, cx, cy - 1, bg);
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx + 1, cy - 1, bg);
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx - 1, cy, bg);
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx + 2, cy, bg);
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx - 1, cy + 1, bg);
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx + 2, cy + 1, bg);
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx, cy + 2, bg);
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx + 1, cy + 2, bg);

    DisplayBitGrid::put_pixel_clipped_raw(layer, cx, cy, fg);
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx + 1, cy, fg);
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx, cy + 1, fg);
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx + 1, cy + 1, fg);
}

/// `PixelWriter` adapter over a raw `*mut DisplayBitGrid`.
struct BitGridWriter(*mut DisplayBitGrid);

impl line_draw::PixelWriter for BitGridWriter {
    #[inline]
    fn put_pixel_clipped(&mut self, x: i32, y: i32, color: u8) {
        unsafe { DisplayBitGrid::put_pixel_clipped_raw(self.0, x, y, color) }
    }
    #[inline]
    fn fill_hline(&mut self, x1: i32, x2: i32, y: i32, color: u8) {
        unsafe { DisplayBitGrid::fill_hline_raw(self.0, x1, x2, y, color) }
    }
    #[inline]
    fn clip_left(&self) -> i32 {
        unsafe { (*self.0).clip_left as i32 }
    }
    #[inline]
    fn clip_top(&self) -> i32 {
        unsafe { (*self.0).clip_top as i32 }
    }
    #[inline]
    fn clip_right(&self) -> i32 {
        unsafe { (*self.0).clip_right as i32 }
    }
    #[inline]
    fn clip_bottom(&self) -> i32 {
        unsafe { (*self.0).clip_bottom as i32 }
    }
}

/// Port of DisplayGfx::DrawLine (slot 13, 0x56BDB0).
pub unsafe extern "thiscall" fn draw_line(
    this: *mut DisplayGfx,
    x1: Fixed,
    y1: Fixed,
    x2: Fixed,
    y2: Fixed,
    color1: u32,
    color2: u32,
) {
    let cam_x = Fixed::from_int((*this).camera_x);
    let cam_y = Fixed::from_int((*this).camera_y);

    acquire_render_lock(this);

    let mut writer = BitGridWriter((*this).layer_0);
    line_draw::draw_line_two_color(
        &mut writer,
        x1 + cam_x,
        y1 + cam_y,
        x2 + cam_x,
        y2 + cam_y,
        color1 as u8,
        color2 as u8,
    );
}

/// Port of DisplayGfx::DrawLineClipped (slot 14, 0x56BD50).
pub unsafe extern "thiscall" fn draw_line_clipped(
    this: *mut DisplayGfx,
    x1: Fixed,
    y1: Fixed,
    x2: Fixed,
    y2: Fixed,
    color: u32,
) {
    let cam_x = Fixed::from_int((*this).camera_x);
    let cam_y = Fixed::from_int((*this).camera_y);

    acquire_render_lock(this);

    let mut writer = BitGridWriter((*this).layer_0);
    line_draw::draw_line_clipped(
        &mut writer,
        x1 + cam_x,
        y1 + cam_y,
        x2 + cam_x,
        y2 + cam_y,
        color as u8,
    );
}

/// Port of DisplayGfx::DrawPolyline (slot 12, 0x56BCC0).
///
/// Points are Fixed-point (x, y) pairs.
pub unsafe extern "thiscall" fn draw_polyline(
    this: *mut DisplayGfx,
    points: *mut i32,
    count: i32,
    color: u32,
) {
    let cam_x = Fixed::from_int((*this).camera_x);
    let cam_y = Fixed::from_int((*this).camera_y);

    let n = count as usize;
    if n == 0 || n > 256 {
        return;
    }

    let mut verts = [Vertex::new(Fixed::ZERO, Fixed::ZERO); 256];
    for (i, vert) in verts.iter_mut().enumerate().take(n) {
        *vert = Vertex::new(
            Fixed::from_raw(*points.add(i * 2)) + cam_x,
            Fixed::from_raw(*points.add(i * 2 + 1)) + cam_y,
        );
    }

    acquire_render_lock(this);

    let mut writer = BitGridWriter((*this).layer_0);
    line_draw::draw_polygon_filled(&mut writer, &verts[..n], color as u8);
}

/// Port of DisplayGfx::IsSpriteLoaded (slot 32, 0x56A480).
///
/// Checks all three sprite arrays (DisplayBase `sprite_ptrs`/`sprite_banks`
/// and DisplayGfx `sprite_table`).
pub unsafe extern "thiscall" fn is_sprite_loaded(this: *mut DisplayGfx, id: i32) -> u32 {
    if !is_valid_sprite_id(id) {
        return 0;
    }

    let base = &(*this).base;
    if !base.sprite_ptrs[id as usize].is_null()
        || !base.sprite_banks[id as usize].is_null()
        || !(*this).sprite_table[id as usize].is_null()
    {
        1
    } else {
        0
    }
}

/// Maximum valid font slot index — `font_table` has 32 entries, slot 0
/// is reserved (the original validates `1..=31` everywhere).
pub const MAX_FONT_ID: i32 = 31;

/// Sentinel bit on the `id` parameter of `load_sprite` / `load_sprite_by_layer`
/// meaning "already loaded — return success without doing any work".
const SPRITE_LOAD_ALREADY_DONE: u32 = 0x0080_0000;

/// Check if a sprite ID is in the valid range `[1, 0x3FF]`. Note that
/// the slot 33 dispatcher uses a slightly tighter bound (`1..=0x3FE`)
/// matching the original — see `get_sprite_frame_for_blit`.
#[inline]
fn is_valid_sprite_id(id: i32) -> bool {
    (1..=0x3FF).contains(&id)
}

/// Check if `font_id` is a valid 1-based index into `font_table`.
#[inline]
fn is_valid_font_id(id: i32) -> bool {
    (1..=MAX_FONT_ID).contains(&id)
}

/// Port of DisplayGfx::GetSpriteInfo (slot 6, 0x523500).
///
/// Returns the static "sprite" string (0x664170) on success, or 0 if
/// `layer` is out of range or no entry exists in `sprite_ptrs` /
/// `sprite_banks`.
pub unsafe extern "thiscall" fn get_sprite_info(
    this: *mut DisplayGfx,
    layer: i32,
    out_data: *mut u32,
    out_flags: *mut u32,
    out_width: *mut u32,
) -> u32 {
    if (layer as u32).wrapping_sub(1) > 0x3FE {
        return 0;
    }

    let base = &(*this).base;

    // Path 1: Sprite* in sprite_ptrs
    let sprite = base.sprite_ptrs[layer as usize];
    if !sprite.is_null() {
        return sprite_info_from_sprite(sprite, out_data, out_flags, out_width);
    }

    // Path 2: SpriteBank* in sprite_banks
    let bank = base.sprite_banks[layer as usize];
    if !bank.is_null() {
        return sprite_info_from_bank(bank, layer, out_data, out_flags, out_width);
    }

    0
}

/// Address of the static "sprite" string in WA.exe .rdata, returned as
/// a type-tag by `get_sprite_info` and friends.
const SPRITE_STRING: u32 = va::STR_SPRITE;

/// Port of Sprite__GetInfo (0x4FAEC0; usercall EAX=this, ESI=out_data,
/// ECX=out_width, stack=out_flags).
unsafe fn sprite_info_from_sprite(
    sprite: *const Sprite,
    out_data: *mut u32,
    out_flags: *mut u32,
    out_width: *mut u32,
) -> u32 {
    let s = &*sprite;

    if s.frame_meta_ptr.is_null() {
        return 0;
    }

    *out_data = (s._unknown_08 as u32) | ((s.fps as u32) << 16);

    // Ping-pong sprites (flags bit 1) report a doubled-minus-one width.
    let mut width = s.max_frames as u32;
    if s.flags & 2 != 0 {
        width = width * 2 - 1;
    }
    *out_width = width;

    *out_flags = (s.flags & 1) as u32;

    crate::rebase::rb(SPRITE_STRING)
}

/// Port of SpriteBank__GetInfo (0x4F98C0; usercall EAX=layer, ECX=this,
/// ESI=out_width, stack=out_data+out_flags).
unsafe fn sprite_info_from_bank(
    bank: *const SpriteBank,
    layer: i32,
    out_data: *mut u32,
    out_flags: *mut u32,
    out_width: *mut u32,
) -> u32 {
    let b = &*bank;

    if b.frame_table.is_null() {
        return 0;
    }

    let entry_idx = *b.index_table.offset((layer - b.base_id) as isize);
    if entry_idx < 0 || entry_idx >= b.frame_count {
        return 0;
    }

    let frame = &*b.frame_table.add(entry_idx as usize);

    *out_data = (frame.data_value as u32) << 8;
    *out_flags = (frame.flags & 1) as u32;

    // GetInfo's "API width" is read from `scale_or_count` (the +8 field),
    // not the per-frame source width at +2 — see the original disassembly
    // at 0x4F98C0 which reads `*(ushort *)(entry + 8)`.
    if frame.scale_or_count & 0x8000 != 0 {
        *out_width = 1;
    } else if frame.flags & 2 != 0 {
        *out_width = (frame.scale_or_count as u32) * 2 - 1;
    } else {
        *out_width = frame.scale_or_count as u32;
    }

    crate::rebase::rb(SPRITE_STRING)
}

/// Pure-Rust port of `Sprite__GetFrameForBlit` (0x4FAD30).
///
/// Original is usercall (`ESI=this, EAX=out_anim_frac, EDX=anim_value,
/// stack=(out_w, out_h, out_left, out_top, out_right, out_bottom)`); the
/// slot 33 dispatcher shuffles its args into this regular signature.
unsafe fn sprite_get_frame_for_blit(
    sprite: *mut Sprite,
    mut anim_value: u32,
    out_anim_frac: *mut u32,
    out_w: *mut i32,
    out_h: *mut i32,
    out_left: *mut i32,
    out_top: *mut i32,
    out_right: *mut i32,
    out_bottom: *mut i32,
) -> *mut DisplayBitGrid {
    let flags = (*sprite).flags;

    // Clamp anim_value: bit 0 = use-as-is (truncate), else signed clamp.
    if flags & 1 != 0 {
        anim_value &= 0xFFFF;
    } else {
        let signed = anim_value as i32;
        if signed < 0 {
            anim_value = 0;
        } else if signed >= 0x10000 {
            anim_value = 0xFFFF;
        }
    }

    // Ping-pong (bounce) iteration.
    if flags & 2 != 0 {
        anim_value &= 0xFFFF;
        anim_value = anim_value.wrapping_mul(2);
        if anim_value >= 0x10000 {
            anim_value = 0x1FFFE - anim_value;
        }
    }

    let frame_idx: u32 = if (*sprite).is_scaled != 0 {
        // Scaled mode: Fixed16 lerp between scale_x/scale_y by anim_value,
        // result written to *out_anim_frac. Original asm:
        // `IMUL EDX; SHRD EAX, EDX, 0x10` (32×32 → 64 signed mul, >> 16).
        let scale_x = (*sprite).scale_x as i32;
        let scale_y = (*sprite).scale_y as i32;
        let diff = scale_y.wrapping_sub(scale_x);
        let prod = (diff as i64).wrapping_mul((anim_value as i32) as i64);
        let interp = ((prod >> 16) as i32).wrapping_add(scale_x);
        *out_anim_frac = interp as u32;
        0
    } else if (*sprite).frame_round_mode & 1 != 0 {
        // Round-to-nearest, with `frame_idx == max_frames` wrapping to 0
        // to avoid reading past the table.
        let max_frames = (*sprite).max_frames as i32;
        let prod = max_frames.wrapping_mul(anim_value as i32);
        let f = (prod.wrapping_add(0x8000) >> 16) as u32;
        *out_anim_frac = 0;
        if f == max_frames as u32 {
            0
        } else {
            f
        }
    } else {
        let max_frames = (*sprite).max_frames as i32;
        let prod = max_frames.wrapping_mul(anim_value as i32);
        *out_anim_frac = 0;
        (prod >> 16) as u32
    };

    let frame_meta = (*sprite).frame_meta_ptr.add(frame_idx as usize);
    let start_x = (*frame_meta).start_x as i16 as i32;
    let start_y = (*frame_meta).start_y as i16 as i32;
    let end_x = (*frame_meta).end_x as i16 as i32;
    let end_y = (*frame_meta).end_y as i16 as i32;
    *out_left = start_x;
    *out_top = start_y;
    *out_right = end_x;
    *out_bottom = end_y;
    *out_w = (*sprite).width as i32;
    *out_h = (*sprite).height as i32;
    let frame_w = end_x - start_x;
    let frame_h = end_y - start_y;

    // Resolve surface address: flat (already-decoded pixels in the load
    // buffer) or cached/decompressed (lazy via FrameCache + LZSS).
    let surface_addr: *mut u8 = if (*sprite).header_flags & 0x4000 == 0 {
        let bitmap_offset = (*frame_meta).bitmap_offset;
        (*sprite).bitmap_data_ptr.add(bitmap_offset as usize)
    } else {
        // Cached path: bitmap_offset is split into a signed-byte
        // subframe index (high byte) and a pixel offset (low 24 bits).
        // The original uses `MOVSX byte ptr [EAX+EDI+3]` + a *12 lea —
        // negative subframe indices index *backward* from the table base.
        let bitmap_offset = (*frame_meta).bitmap_offset;
        let subframe_idx_signed = ((bitmap_offset >> 24) as i8) as i32;
        let entry = (*sprite)
            .subframe_cache_table
            .offset(subframe_idx_signed as isize);

        if (*entry).decoded_ptr.is_null() {
            let context_ptr = (*sprite).context_ptr;
            let decoded_size = (*entry).decoded_size;
            let decoded = frame_cache_allocate(
                decoded_size,
                context_ptr,
                sprite as *mut core::ffi::c_void,
                subframe_idx_signed as u32,
            );
            (*entry).decoded_ptr = decoded;
            let src = (*sprite)
                .bitmap_data_ptr
                .add((*entry).compressed_offset as usize);
            sprite_lzss_decode(decoded, src, (*sprite).palette_data_ptr);
        }

        let pixel_offset = (bitmap_offset & 0xFF_FFFF) as usize;
        (*entry).decoded_ptr.add(pixel_offset)
    };

    // Update embedded bitgrid only if it owns an external buffer slot.
    // Field write order copied from 0x4FAEA2..0x4FAEB7; equivalent to
    // the shared `DisplayBitGrid::SetExternalBuffer` helper that the
    // SpriteBank path calls.
    let bitgrid = &raw mut (*sprite).bitgrid;
    if (*bitgrid).external_buffer != 0 {
        (*bitgrid).clip_bottom = frame_h as u32;
        (*bitgrid).clip_right = frame_w as u32;
        (*bitgrid).clip_top = 0;
        (*bitgrid).clip_left = 0;
        (*bitgrid).row_stride = frame_w as u32;
        (*bitgrid).data = surface_addr;
        (*bitgrid).height = frame_h as u32;
        (*bitgrid).width = frame_w as u32;
    }

    bitgrid
}

/// Panic stub for `SpriteBank__GetFrameForBlit` (0x4F9710).
///
/// Structurally unreachable in shipping WA: the only `SpriteBank`
/// constructor is `SpriteBank__Constructor` (0x4F9450), reached only
/// via `LoadSpriteEx` (slot 30, 0x523310), which is itself trapped in
/// `install_display`. So `sprite_banks[id]` is always null and the bank
/// branch of slot 33 is dead code — confirmed by playing several turns
/// without firing this panic. A disassembly-derived port lived in
/// commit `973f234`; per the no-unverified-code rule it was deleted.
/// Revive from history if banks ever become live.
#[allow(clippy::too_many_arguments)]
unsafe fn sprite_bank_get_frame_for_blit(
    _bank: *mut SpriteBank,
    sprite_id: u32,
    _anim_value: u32,
    _out_anim_frac: *mut u32,
    _out_w: *mut i32,
    _out_h: *mut i32,
    _out_left: *mut i32,
    _out_top: *mut i32,
    _out_right: *mut i32,
    _out_bottom: *mut i32,
) -> *mut DisplayBitGrid {
    panic!(
        "SpriteBank::GetFrameForBlit reached for sprite_id={sprite_id} — \
         banks were supposed to be unreachable. Revive the port from \
         commit 973f234 and validate against 0x4F9710."
    );
}

/// Port of `DisplayGfx::GetSpriteFrameForBlit` (slot 33, 0x5237C0).
///
/// Thin dispatcher: forwards `sprite_ptrs`-backed IDs to
/// [`sprite_get_frame_for_blit`] and `sprite_banks`-backed IDs to
/// [`sprite_bank_get_frame_for_blit`] (a panic stub).
#[allow(clippy::too_many_arguments)]
pub unsafe extern "thiscall" fn get_sprite_frame_for_blit(
    this: *mut DisplayGfx,
    sprite_id: u32,
    anim_value: u32,
    out_w: *mut i32,
    out_h: *mut i32,
    out_left: *mut i32,
    out_top: *mut i32,
    out_right: *mut i32,
    out_bottom: *mut i32,
    out_anim_frac: *mut u32,
) -> *mut DisplayBitGrid {
    if sprite_id.wrapping_sub(1) >= 0x3FE {
        return core::ptr::null_mut();
    }

    let base = &mut (*this).base;
    let sprite = base.sprite_ptrs[sprite_id as usize];
    if !sprite.is_null() {
        return sprite_get_frame_for_blit(
            sprite,
            anim_value,
            out_anim_frac,
            out_w,
            out_h,
            out_left,
            out_top,
            out_right,
            out_bottom,
        );
    }

    let bank = base.sprite_banks[sprite_id as usize];
    if !bank.is_null() {
        return sprite_bank_get_frame_for_blit(
            bank,
            sprite_id,
            anim_value,
            out_anim_frac,
            out_w,
            out_h,
            out_left,
            out_top,
            out_right,
            out_bottom,
        );
    }

    core::ptr::null_mut()
}

/// Port of DisplayGfx::DrawViaCallback (slot 21, 0x56B7C0).
///
/// Calls `obj->vtable[2](layer_0, pixel_x, pixel_y, p5, p6)` with
/// camera-adjusted coordinates.
pub unsafe extern "thiscall" fn draw_via_callback(
    this: *mut DisplayGfx,
    x: Fixed,
    y: Fixed,
    obj: *mut u8,
    p5: u32,
    p6: u32,
) {
    acquire_render_lock(this);

    let pixel_x = (Fixed::from_int((*this).camera_x) + x).to_int();
    let pixel_y = (Fixed::from_int((*this).camera_y) + y).to_int();
    let layer_0 = (*this).layer_0;

    let vtable = *(obj as *const *const u32);
    let callback: unsafe extern "thiscall" fn(*mut u8, *mut DisplayBitGrid, i32, i32, u32, u32) =
        core::mem::transmute(*vtable.add(2));
    callback(obj, layer_0, pixel_x, pixel_y, p5, p6);
}

/// Port of DisplayGfx::DrawTiledTerrain (slot 22, 0x56C5A0).
///
/// Tiles `tile_bitmap_sets[1]`'s bitmaps in a row-major grid. `count`
/// limits how many pixel-rows are rendered. `flags` low 16 bits must be 1
/// (only supported mode); bit 19 controls a blit transparency flag.
pub unsafe extern "thiscall" fn draw_tiled_terrain(
    this: *mut DisplayGfx,
    x: Fixed,
    y: Fixed,
    mut count: i32,
    flags: u32,
) {
    if count <= 0 {
        return;
    }

    let total_height = (*this).tile_total_height;
    if count > total_height {
        count = total_height;
    }

    if (flags & 0xFFFF) != 1 {
        return;
    }

    let tile_set = (*this).tile_bitmap_sets[1];
    if tile_set.is_null() {
        return;
    }

    let bitmap_array = (*tile_set).bitmap_ptrs;

    let pixel_x = (*this).camera_x + x.to_int();
    let pixel_y = (*this).camera_y + y.to_int();
    let blit_flags = (!flags >> 19) & 2;

    let total_width = (*this).tile_total_width;
    let col_width = (*this).tile_col_width;
    let row_height = (*this).tile_row_height;

    let mut bitmap_idx = 0u32;
    let mut y_offset = 0i32;

    if total_height <= 0 {
        return;
    }

    while y_offset < total_height {
        if y_offset >= count {
            return;
        }

        let mut row_h = row_height;
        if count - y_offset < row_height {
            row_h = count - y_offset;
        }

        let mut x_offset = 0i32;
        while x_offset < total_width {
            let col_w = col_width.min(total_width - x_offset);

            let bitmap_ptr = *bitmap_array.add(bitmap_idx as usize);

            blit_bitmap_clipped_native(
                this,
                pixel_x + x_offset,
                pixel_y + y_offset,
                col_w,
                row_h,
                bitmap_ptr,
                blit_flags,
            );

            bitmap_idx += 1;
            x_offset += col_width;
        }

        y_offset += row_height;
    }
}

/// Port of DisplayGfx::DrawScaledSprite (slot 20, 0x56B660).
///
/// The source rect is `(src_x, src_y)..(src_w, src_h)`. Flags select
/// the blit mode:
/// - bit 20: 0 = ColorTable (transparency), 1 = Copy (opaque)
/// - 0x200000: additive blend via `color_add_table` LUT
/// - 0x4000000: color blend via `color_blend_table` LUT
/// - 0x8000000 / 0x10000000: stippled (checkerboard) blit
pub unsafe fn draw_scaled_sprite(
    this: *mut DisplayGfx,
    x: Fixed,
    y: Fixed,
    sprite: *mut DisplayBitGrid,
    src_x: i32,
    src_y: i32,
    src_w: i32,
    src_h: i32,
    flags: u32,
) -> DrawScaledSpriteResult {
    let width = src_w - src_x;
    let height = src_h - src_y;

    // Signed division rounding toward zero.
    let half_w = if width < 0 {
        (width + 1) / 2
    } else {
        width / 2
    };
    let half_h = if height < 0 {
        (height + 1) / 2
    } else {
        height / 2
    };

    let dst_x = (*this).camera_x - half_w + (x.0 >> 16);
    let dst_y = (*this).camera_y - half_h + (y.0 >> 16);

    let blend_mode = (!(flags >> 20)) & 1;

    if (flags & 0x8000000) != 0 || (flags & 0x10000000) != 0 {
        let stipple_mode: u32 = if (flags & 0x10000000) != 0 { 1 } else { 0 };
        return DrawScaledSpriteResult::Stippled {
            layer: (*this).layer_0,
            dst_x,
            dst_y,
            width,
            height,
            sprite,
            src_x,
            src_y,
            stipple_mode,
        };
    }

    let color_table: *const u8 = if (flags & 0x200000) != 0 {
        (*this).color_add_table.as_ptr()
    } else if (flags & 0x4000000) != 0 {
        (*this).color_blend_table.as_ptr()
    } else {
        core::ptr::null()
    };

    if width <= 0 || height <= 0 {
        return DrawScaledSpriteResult::Handled;
    }

    acquire_render_lock(this);

    let layer = (*this).layer_0;

    // Core blit flags: low 16 bits = blend mode (0 = Copy, 1 = ColorTable).
    let blit_flags = blend_mode;

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
    }
}

/// Result of `draw_scaled_sprite`'s coordinate / mode resolution. The
/// actual blit is performed by the DLL hook layer, which has access to
/// `blit_impl` (the bridge from `DisplayBitGrid` to `PixelGrid`).
pub enum DrawScaledSpriteResult {
    /// Blit should be performed with these parameters.
    Blit {
        layer: *mut DisplayBitGrid,
        dst_x: i32,
        dst_y: i32,
        width: i32,
        height: i32,
        sprite: *mut DisplayBitGrid,
        src_x: i32,
        src_y: i32,
        color_table: *const u8,
        blit_flags: u32,
    },
    /// Stippled (checkerboard) blit — caller should use blit_stippled_raw.
    Stippled {
        layer: *mut DisplayBitGrid,
        dst_x: i32,
        dst_y: i32,
        width: i32,
        height: i32,
        sprite: *mut DisplayBitGrid,
        src_x: i32,
        src_y: i32,
        stipple_mode: u32,
    },
    /// Already handled (e.g. zero-size, early out).
    Handled,
}

/// Port of DisplayGfx::DrawPixelStrip (slot 15, 0x56BE10).
pub unsafe extern "thiscall" fn draw_pixel_strip(
    this: *mut DisplayGfx,
    x: Fixed,
    y: Fixed,
    dx: Fixed,
    dy: Fixed,
    count: i32,
    color: u32,
) {
    let mut cx = Fixed::from_int((*this).camera_x) + x;
    let mut cy = Fixed::from_int((*this).camera_y) + y;

    acquire_render_lock(this);

    let layer = (*this).layer_0;
    if count >= 0 {
        for _ in 0..=count {
            DisplayBitGrid::put_pixel_clipped_raw(layer, cx.to_int(), cy.to_int(), color as u8);
            cx += dx;
            cy += dy;
        }
    }
}

/// Port of DisplayGfx::SetLayerColor (slot 4, 0x5231E0).
///
/// Allocates a `PaletteContext` for `layer` (1-3) if one doesn't exist,
/// claiming `color` consecutive entries in the slot table.
pub unsafe extern "thiscall" fn set_layer_color(this: *mut DisplayGfx, layer: i32, color: i32) {
    let Some(layer) = Layer::try_from_i32(layer) else {
        return;
    };

    if !(*this).base.layer_contexts[layer.idx()].is_null() {
        return;
    }

    let start = palette_slot_alloc(&mut (*this).base, color);

    // PaletteContext is 0x72C bytes; only the first 0x70C is zeroed.
    let ctx = crate::wa_alloc::wa_malloc(0x72C);
    core::ptr::write_bytes(ctx, 0, 0x70C);

    let result = if ctx.is_null() {
        core::ptr::null_mut()
    } else {
        let ctx = ctx as *mut PaletteContext;
        palette_context_init(ctx, start as i16, (start as i32 + color - 1) as i16);
        ctx
    };

    (*this).base.layer_contexts[layer.idx()] = result;
    (*this).base.layer_visibility[layer.idx()] = 0;
}

/// Palette slot allocator — port of FUN_00523190.
///
/// Scans the 257-entry table `slot_table_guard(0) + slot_table[1..=255] +
/// slot_table_sentinel(-1)` for `count` consecutive available (non-zero)
/// entries, zeroes them, and returns the start index. Returns -1 on
/// failure (the sentinel is hit before `count` consecutive slots).
unsafe fn palette_slot_alloc(base: &mut DisplayBase<*const DisplayGfxVtable>, count: i32) -> i32 {
    let count = count as usize;
    let table = &base.slot_table_guard as *const u32;
    let total = 1 + 0xFF + 1;

    let mut consecutive = 0usize;
    let mut scan = 0usize;

    loop {
        if consecutive == count {
            let start = scan - count;
            for i in start..scan {
                *(table.add(i) as *mut u32) = 0;
            }
            return start as i32;
        }
        if scan >= total {
            return -1;
        }
        let val = *table.add(scan) as i32;
        scan += 1;
        if val == 0 {
            consecutive = 0;
        } else if val < 0 {
            return -1;
        } else {
            consecutive += 1;
        }
    }
}

/// Initialize a PaletteContext with a palette index range —
/// port of FUN_00541170 + FUN_005411A0.
unsafe fn palette_context_init(ctx: *mut PaletteContext, range_min: i16, range_max: i16) {
    (*ctx).dirty_range_min = range_min;
    (*ctx).dirty_range_max = range_max;

    let range_size = range_max - range_min + 1;
    (*ctx).cache_count = 0;
    (*ctx).free_count = range_size;

    // Fill free_stack with [range_max, range_max-1, ..., range_min].
    if range_size > 0 {
        for i in 0..range_size as usize {
            (*ctx).free_stack[i] = (range_max as u8).wrapping_sub(i as u8);
        }
    }

    (*ctx).cache_iter = 0;
    core::ptr::write_bytes((*ctx).in_use.as_mut_ptr(), 0, 256);

    (*ctx).dirty = 0;
}

/// Port of DisplayGfx::SetActiveLayer (slot 5, 0x523270).
///
/// Returns the `PaletteContext*` for `layer` (1-3), or null if out of
/// range. Used as palette data input for `update_palette`.
pub unsafe extern "thiscall" fn set_active_layer(
    this: *mut DisplayGfx,
    layer: i32,
) -> *mut PaletteContext {
    match Layer::try_from_i32(layer) {
        Some(layer) => (*this).base.layer_contexts[layer.idx()],
        None => core::ptr::null_mut(),
    }
}

/// Port of DisplayGfx::UpdatePalette (slot 24, 0x56A610).
///
/// Copies the indices listed in `palette_ctx.cache` from its `rgb_table`
/// into DisplayGfx's `palette_entries`. Pushes the result to the DDraw
/// surface palette via [`palette_commit`] if `commit != 0`.
pub unsafe extern "thiscall" fn update_palette(
    this: *mut DisplayGfx,
    palette_ctx: *mut PaletteContext,
    commit: i32,
) {
    let ctx = &mut *palette_ctx;

    ctx.cache_iter = 0;

    if ctx.cache_count <= 0 {
        return;
    }

    let dirty_min = ctx.dirty_range_min as i32;
    let dirty_max = ctx.dirty_range_max as i32;

    ctx.cache_iter = 1;
    let mut idx = ctx.cache[0] as usize;

    loop {
        // rgb_table[idx] is a packed u32: low 3 bytes = R, G, B.
        let rgb = ctx.rgb_table[idx].to_le_bytes();
        (*this).palette_entries[idx * 4] = rgb[0];
        (*this).palette_entries[idx * 4 + 1] = rgb[1];
        (*this).palette_entries[idx * 4 + 2] = rgb[2];
        (*this).palette_entries[idx * 4 + 3] = 0;

        if ctx.cache_iter >= ctx.cache_count {
            break;
        }
        idx = ctx.cache[ctx.cache_iter as usize] as usize;
        ctx.cache_iter += 1;
    }

    // Expand the dirty palette range to cover this update.
    if ((*this).palette_dirty_min as i32) > dirty_min {
        (*this).palette_dirty_min = dirty_min as u32;
    }
    if ((*this).palette_dirty_max as i32) < dirty_max {
        (*this).palette_dirty_max = dirty_max as u32;
    }

    if commit != 0 {
        palette_commit(this);
        (*this).palette_dirty_min = 0x100;
        (*this).palette_dirty_max = 0xFFFF_FFFF;
    }
}

/// Call WA's palette commit function (0x56CD20). Usercall:
/// `EAX=dirty_min, EDX=dirty_max, stack=this (DisplayGfx*)`.
unsafe fn palette_commit(gfx: *mut DisplayGfx) {
    let dirty_min = (*gfx).palette_dirty_min;
    let dirty_max = (*gfx).palette_dirty_max;
    palette_commit_bridge(gfx, dirty_min, dirty_max, crate::rebase::rb(0x0056_CD20));
}

#[unsafe(naked)]
unsafe extern "cdecl" fn palette_commit_bridge(
    _gfx: *mut DisplayGfx,
    _dirty_min: u32,
    _dirty_max: u32,
    _target: u32,
) {
    core::arch::naked_asm!(
        "mov eax, [esp + 8]",        // dirty_min
        "mov edx, [esp + 12]",       // dirty_max
        "push dword ptr [esp + 4]",  // gfx (this)
        "call dword ptr [esp + 20]", // target (+4 from our push, +16 from original)
        "ret",
    );
}

/// Port of DisplayGfx::SetLayerVisibility (slot 23, 0x56A5D0).
///
/// Updates the palette from the layer's context (if it exists) and
/// clears the layer's visibility flag when `visible < 0`.
pub unsafe extern "thiscall" fn set_layer_visibility(
    this: *mut DisplayGfx,
    layer: i32,
    visible: i32,
) {
    let Some(layer) = Layer::try_from_i32(layer) else {
        return;
    };

    let layer_ctx = (*this).base.layer_contexts[layer.idx()];
    if !layer_ctx.is_null() {
        update_palette(this, layer_ctx, visible);
    }

    if visible < 0 {
        (*this).base.layer_visibility[layer.idx()] = 0;
    }
}

// =========================================================================
// Sprite loading methods
// =========================================================================

/// Pure-Rust port of `ConstructSprite` (0x4FAA30). The caller must
/// pre-zero the rest of the `Sprite` allocation.
pub unsafe fn construct_sprite(sprite: *mut Sprite, sprite_cache: *mut SpriteCache) {
    use crate::bitgrid::{BitGridDisplayVtable, BIT_GRID_DISPLAY_VTABLE};
    use crate::rebase::rb;

    (*sprite).vtable = rb(va::SPRITE_VTABLE) as *const SpriteVtable;
    (*sprite).context_ptr = sprite_cache;

    (*sprite).bitgrid.vtable = rb(BIT_GRID_DISPLAY_VTABLE) as *const BitGridDisplayVtable;
    (*sprite).bitgrid.external_buffer = 1;
    (*sprite).bitgrid.cells_per_unit = 8;
}

/// Port of DisplayGfx::LoadSprite (slot 31, 0x523400).
///
/// `flag != 0` clamps `max_frames` to the low 16 bits of `id` and uses
/// the high 16 bits to seed the per-sprite frame round mode bytes.
pub unsafe fn load_sprite(
    this: *mut DisplayGfx,
    layer: u32,
    id: u32,
    flag: u32,
    gfx_dir: *mut GfxDir,
    _name: *const c_char,
    load_sprite_from_vfs: unsafe extern "cdecl" fn(
        sprite: *mut Sprite,
        gfx_dir: *mut GfxDir,
        name: *const c_char,
        layer_ctx: *mut PaletteContext,
    ) -> i32,
) -> i32 {
    if id & SPRITE_LOAD_ALREADY_DONE != 0 {
        return 1;
    }

    let Some(layer) = Layer::try_from_u32(layer) else {
        return 0;
    };
    let base = &mut (*this).base;
    let layer_ctx = base.layer_contexts[layer.idx()];
    if layer_ctx.is_null() {
        return 0;
    }
    if !is_valid_sprite_id(id as i32) {
        return 0;
    }

    if is_sprite_loaded(this, id as i32) != 0 {
        return 0;
    }

    let sprite = wa_malloc_struct_zeroed::<Sprite>();
    if sprite.is_null() {
        return 0;
    }
    construct_sprite(sprite, base.sprite_cache);

    let result = load_sprite_from_vfs(sprite, gfx_dir, _name, layer_ctx);
    if result == 0 {
        if !sprite.is_null() {
            let dtor = (*(*sprite).vtable).destructor;
            dtor(sprite, 1);
        }
        return 0;
    }

    let base = &mut (*this).base;
    base.sprite_ptrs[id as usize] = sprite;
    base.sprite_layers[id as usize] = layer.as_u32();
    base.layer_visibility[layer.idx()] += 1;

    if flag != 0 {
        let sprite = base.sprite_ptrs[id as usize];
        let id_u16 = id as u16;
        if id_u16 != 0 && id_u16 < (*sprite).max_frames {
            (*sprite).max_frames = id_u16;
        }
        // Original splits the high word of `id` into the rounding-mode
        // byte (bit 0 picks the round-to-nearest path in
        // `Sprite__GetFrameForBlit`) and an adjacent unknown byte.
        (*sprite).frame_round_mode = (id >> 16) as u8;
        (*sprite)._unknown_19 = (id >> 24) as u8;
    }

    1
}

/// Port of FUN_005733B0 (`LoadSpriteByName`; original is usercall
/// `EDI=sprite, ECX=gfx_dir, stack=(palette_ctx, name), RET 0x8`).
///
/// Reads the sprite header, palette, and frame pixel data from a `.dir`
/// archive stream. In headless mode (`g_DisplayModeFlag != 0`) skips all
/// surface creation.
pub unsafe fn load_sprite_by_name(
    sprite: *mut LayerSprite,
    gfx_dir: *mut GfxDir,
    palette_ctx: *mut PaletteContext,
    name: *const c_char,
) -> i32 {
    use crate::address::va;
    use crate::rebase::rb;
    use crate::render::display::context::{FastcallResult, RenderContext, Surface};
    use crate::render::palette::{palette_map_color, remap_pixels_through_lut};
    use crate::render::sprite::gfx_dir::{call_gfx_load_image, GfxDirStream};

    use crate::wa_alloc::wa_malloc;

    // Copy name into sprite.name (max 0x4F chars + null terminator).
    let name_dest = (*sprite).name.as_mut_ptr();
    let mut i = 0usize;
    while i < 0x4F {
        let ch = *name.add(i);
        *name_dest.add(i) = ch as u8;
        if ch == 0 {
            break;
        }
        i += 1;
    }
    *name_dest.add(i.min(0x4F)) = 0;

    (*sprite).gfx_dir = gfx_dir;
    (*sprite).palette_ctx = palette_ctx;

    let stream = call_gfx_load_image(gfx_dir, name);
    if stream.is_null() {
        return 0;
    }

    let display_mode_flag = *(rb(va::G_DISPLAY_MODE_FLAG) as *const u8);
    if display_mode_flag == 0 {
        // The original calls remaining() and discards the result.
        GfxDirStream::remaining_raw(stream);

        // Read the .spr header as 4+4+2+2 separate calls — matching the
        // original's read granularity may matter for internal stream state.
        let mut hdr4 = [0u8; 4];
        GfxDirStream::read_raw(stream, hdr4.as_mut_ptr(), 4); // unused/version
        GfxDirStream::read_raw(stream, hdr4.as_mut_ptr(), 4); // data_size

        let mut header_flags: u16 = 0;
        GfxDirStream::read_raw(stream, &mut header_flags as *mut u16 as *mut u8, 2);

        let mut palette_count: u32 = 0;
        GfxDirStream::read_raw(stream, &mut palette_count as *mut u32 as *mut u8, 2);

        // Build palette LUT: bulk-read all RGB triplets then iterate.
        // Palette entry 0 is always transparent — the file's RGB data
        // defines entries 1..=palette_count, not entry 0. So
        // palette_data[0..3] maps to lut[1], not lut[0].
        let mut palette_lut = [0u8; 256];
        let lut_count = (palette_count as usize).min(256);
        let bulk_size = palette_count as usize * 3;
        let mut palette_data = [0u8; 768];
        GfxDirStream::read_raw(stream, palette_data.as_mut_ptr(), bulk_size as u32);
        palette_lut[0] = 0;
        for idx in 0..lut_count {
            let r = palette_data[idx * 3];
            let g = palette_data[idx * 3 + 1];
            let b = palette_data[idx * 3 + 2];

            if r == 0 && g == 0 && b == 0 {
                palette_lut[idx + 1] = 0;
            } else {
                let rgb = (r as u32) | ((g as u32) << 8) | ((b as u32) << 16);
                let mapped = palette_map_color(palette_ctx, rgb);
                palette_lut[idx + 1] = mapped as u8;
            }
        }

        // Sprite metadata fields, in the original's read order
        // (field_60 before flags/cell_width/cell_height).
        GfxDirStream::read_raw(stream, &raw mut (*sprite).field_60 as *mut u8, 4);
        GfxDirStream::read_raw(stream, &raw mut (*sprite).flags as *mut u8, 2);
        GfxDirStream::read_raw(stream, &raw mut (*sprite).cell_width as *mut u8, 2);
        GfxDirStream::read_raw(stream, &raw mut (*sprite).cell_height as *mut u8, 2);

        (*sprite).frame_count = 0;
        GfxDirStream::read_raw(stream, &raw mut (*sprite).frame_count as *mut u8, 2);
        let frame_count = (*sprite).frame_count as usize;

        // Counted `LayerSpriteFrame` array: `count * 0x14` bytes plus a
        // 4-byte count prefix at `[-4]`. Saturate the allocation size on
        // overflow to match the original's checked-mul behavior.
        const LSF_SIZE: u32 = core::mem::size_of::<LayerSpriteFrame>() as u32;
        let checked_count = frame_count as u32;
        let checked_size = checked_count.checked_mul(LSF_SIZE).unwrap_or(u32::MAX);
        let checked_alloc = checked_size.checked_add(4).unwrap_or(u32::MAX);

        let array_base = wa_malloc(checked_alloc);
        let frame_array: *mut LayerSpriteFrame = if !array_base.is_null() {
            *(array_base as *mut u32) = checked_count;
            let arr = array_base.add(4) as *mut LayerSpriteFrame;
            let cbitmap_vt = rb(va::CBITMAP_VTABLE_MAYBE) as *const core::ffi::c_void;
            for j in 0..frame_count {
                let elem = arr.add(j);
                (*elem).bitmap_vtable = cbitmap_vt;
                (*elem).surface = core::ptr::null_mut();
            }
            arr
        } else {
            core::ptr::null_mut()
        };
        (*sprite).frame_array = frame_array;

        // Skip alignment padding: while (remaining() & 3) != 0, read 1 byte.
        loop {
            let remaining = GfxDirStream::remaining_raw(stream);
            if remaining & 3 == 0 {
                break;
            }
            let mut dummy = 0u8;
            GfxDirStream::read_raw(stream, &mut dummy, 1);
        }

        // Frame headers: 4-byte discarded prefix, then start_x/y, end_x/y.
        if frame_count > 0 && !frame_array.is_null() {
            for j in 0..frame_count {
                let frame = frame_array.add(j);
                let mut frame_hdr = [0u8; 4];
                GfxDirStream::read_raw(stream, frame_hdr.as_mut_ptr(), 4);
                GfxDirStream::read_raw(stream, &raw mut (*frame).start_x as *mut u8, 2);
                GfxDirStream::read_raw(stream, &raw mut (*frame).start_y as *mut u8, 2);
                GfxDirStream::read_raw(stream, &raw mut (*frame).end_x as *mut u8, 2);
                GfxDirStream::read_raw(stream, &raw mut (*frame).end_y as *mut u8, 2);
            }
        }

        // Per-frame surface creation + pixel data load.
        if frame_count > 0 && !frame_array.is_null() {
            let render_ctx = *(rb(va::G_RENDER_CONTEXT) as *const *mut RenderContext);

            for j in 0..frame_count {
                let frame = frame_array.add(j);
                let width = ((*frame).end_x - (*frame).start_x) as i32;
                let height = ((*frame).end_y - (*frame).start_y) as i32;

                if width * height == 0 {
                    continue;
                }

                // alloc_surface returns the surface pointer in EAX, NOT via
                // the FastcallResult buffer — see feedback_alloc_surface_return.md.
                if (*frame).surface.is_null() {
                    let mut buf = FastcallResult::default();
                    let ret = RenderContext::alloc_surface_raw(render_ctx, &mut buf);
                    (*frame).surface = ret as *mut Surface;
                }
                let surface = (*frame).surface;
                if surface.is_null() {
                    continue;
                }

                let mut buf = FastcallResult::default();
                Surface::init_surface_raw(surface, &mut buf, width, height, 0);
                Surface::set_color_key_raw(surface, &mut buf, 0, 0x10);

                let mut data_ptr: *mut u8 = core::ptr::null_mut();
                let mut pitch: i32 = 0;
                Surface::lock_surface_raw(surface, &mut buf, &mut data_ptr, &mut pitch);

                if !data_ptr.is_null() && pitch != 0 {
                    for row in 0..height {
                        let row_dest = data_ptr.add((row * pitch) as usize);
                        GfxDirStream::read_raw(stream, row_dest, width as u32);
                    }

                    let width_dwords = ((width as u32) + 3) / 4;
                    remap_pixels_through_lut(
                        data_ptr,
                        pitch as u32,
                        palette_lut.as_ptr(),
                        width_dwords,
                        height as u32,
                    );
                }

                Surface::unlock_surface_raw(surface, &mut buf, data_ptr);
            }
        }
    }

    GfxDirStream::destroy_raw(stream);
    1
}

/// Port of FUN_0056A2F0 (`FreeLayerSprite`; usercall EDI=sprite).
pub unsafe fn free_layer_sprite(sprite: *mut LayerSprite) {
    use crate::wa_alloc::wa_free;

    let frame_array = (*sprite).frame_array;
    if !frame_array.is_null() {
        // Counted array: count lives at `frame_array[-4]`.
        let count_ptr = (frame_array as *mut u32).sub(1);
        let count = *count_ptr as usize;

        // Reverse-order destruction matches eh_vector_destructor_iterator.
        for i in (0..count).rev() {
            let frame = frame_array.add(i);
            let surface = (*frame).surface;
            if !surface.is_null() {
                // Surface destructor is vtable[0]; not yet a typed slot.
                let vt = (*surface).vtable as *const usize;
                let dtor: unsafe extern "thiscall" fn(*mut Surface, u32) =
                    core::mem::transmute(*vt);
                dtor(surface, 1);
            }
        }

        wa_free(count_ptr);
    }

    wa_free(sprite);
}

/// Port of DisplayGfx::LoadSpriteByLayer (slot 37, 0x56A4C0).
///
/// Simplified sprite loading that stores into `DisplayGfx::sprite_table`
/// (+0x3DD4) instead of `DisplayBase::sprite_ptrs`. Unlike `load_sprite`
/// it does NOT call `construct_sprite`.
pub unsafe fn load_sprite_by_layer(
    this: *mut DisplayGfx,
    layer: u32,
    id: u32,
    gfx_dir: *mut GfxDir,
    name: *const c_char,
) -> i32 {
    use crate::wa_alloc::wa_malloc_zeroed;

    if id & SPRITE_LOAD_ALREADY_DONE != 0 {
        return 1;
    }

    let palette_ctx = set_active_layer(this, layer as i32);

    if !is_valid_sprite_id(id as i32) {
        return 0;
    }

    if is_sprite_loaded(this, id as i32) != 0 {
        return 1;
    }

    // 0x70 bytes + 0x20 trailing guard, matching WA_MallocMemset.
    let sprite = wa_malloc_zeroed(0x90) as *mut LayerSprite;
    if sprite.is_null() {
        return 0;
    }

    (*sprite).display_gfx = this;
    (*sprite).frame_count = 0;
    (*sprite).frame_array = core::ptr::null_mut();
    (*sprite).gfx_dir = core::ptr::null_mut();

    let result = load_sprite_by_name(sprite, gfx_dir, palette_ctx, name);
    if result == 0 {
        free_layer_sprite(sprite);
        return 0;
    }

    (*this).sprite_table[id as usize] = sprite;

    1
}

/// Port of `DisplayGfx::LoadFont` (slot 34, 0x523560).
///
/// `layer` is the WA "mode" parameter, but it's the same value space as
/// the layer index everywhere else (indexes `layer_contexts[1..=3]` and
/// `layer_visibility[1..=3]`). Shipping WA only ever passes `1`.
///
/// On load failure the partially-initialized font object is leaked, to
/// match the original's call to an unported sprite-bank-style cleanup
/// helper (`FUN_005230C0`).
pub unsafe fn load_font(
    this: *mut DisplayGfx,
    layer: u32,
    font_id: i32,
    gfx_dir: *mut GfxDir,
    filename: *const c_char,
) -> u32 {
    use crate::wa_alloc::wa_malloc_struct_zeroed;

    let Some(layer) = Layer::try_from_u32(layer) else {
        return 0;
    };

    let base = &mut (*this).base;
    let layer_ctx = base.layer_contexts[layer.idx()];
    if layer_ctx.is_null() {
        return 0;
    }

    if !is_valid_font_id(font_id) {
        return 0;
    }
    if !base.font_table[font_id as usize].is_null() {
        return 0;
    }

    let font_obj = wa_malloc_struct_zeroed::<Font>();
    if font_obj.is_null() {
        return 0;
    }

    let result = font_load_from_gfx(font_obj, gfx_dir, layer_ctx, filename);
    if result == 0 {
        // Leak on failure to match the original's behavior.
        return 0;
    }

    base.font_table[font_id as usize] = font_obj;
    base.font_layers[font_id as usize] = layer.as_u32();
    base.layer_visibility[layer.idx()] += 1;

    1
}

/// Port of `DisplayGfx::LoadFontExtension` (slot 35, 0x523620).
pub unsafe fn load_font_extension(
    this: *mut DisplayGfx,
    font_id: i32,
    path: *const c_char,
    char_map: *const c_char,
    palette_value: u32,
    _flag: i32,
) -> u32 {
    use crate::render::palette::palette_context_lookup_entry;

    if !is_valid_font_id(font_id) {
        return 0;
    }
    let base = &mut (*this).base;
    let font_obj_addr = base.font_table[font_id as usize];
    if font_obj_addr.is_null() {
        return 0;
    }
    let font_obj = font_obj_addr as *mut Font;

    // The original ALWAYS resolves the RGB through layer_contexts[1],
    // regardless of which layer owns the font. (Disasm at 0x52364D:
    // `MOV ECX, [EDI+0x3120]` = `layer_contexts[1]`.)
    let layer1_ctx = base.layer_contexts[Layer::ONE.idx()];
    let mut resolved_rgb: u32 = 0;
    let _ = palette_context_lookup_entry(layer1_ctx, palette_value as i32, &mut resolved_rgb);

    // For the extension call we use the font's *owning* layer's palette
    // context. The original reads `layer_contexts[font_layers[font_id]]`
    // without validation, so a zero font_layers entry indexes
    // `layer_contexts[0]` (always null). We preserve that exact lookup.
    let layer_idx = base.font_layers[font_id as usize] as usize;
    let layer_ctx = base.layer_contexts[layer_idx];

    font_extend(font_obj, layer_ctx, path, char_map, resolved_rgb);

    1
}

/// Port of `DisplayGfx::GetFontInfo` (slot 8, 0x523790).
///
/// `out_1` = max metric, `out_2` = font max width — the original passes
/// them via EDI/EDX respectively.
pub unsafe extern "thiscall" fn get_font_info(
    this: *mut DisplayGfx,
    font_id: i32,
    out_1: *mut u32,
    out_2: *mut u32,
) -> u32 {
    if !is_valid_font_id(font_id) {
        return 0;
    }
    let font_obj = (*this).base.font_table[font_id as usize] as *const Font;
    if font_obj.is_null() {
        return 0;
    }
    font_get_info_impl(font_obj, out_1 as *mut i32, out_2 as *mut i32)
}

/// Port of `DisplayGfx::GetFontMetric` (slot 9, 0x523750).
///
/// `out_1` = per-character metric (via EDX), `out_2` = font max width
/// (via EDI). `char_code` is truncated to 8 bits to match `MOV AL, ...`.
pub unsafe extern "thiscall" fn get_font_metric(
    this: *mut DisplayGfx,
    font_id: i32,
    char_code: u32,
    out_1: *mut u32,
    out_2: *mut u32,
) -> u32 {
    if !is_valid_font_id(font_id) {
        return 0;
    }
    let font_obj = (*this).base.font_table[font_id as usize] as *const Font;
    if font_obj.is_null() {
        return 0;
    }
    font_get_metric_impl(
        font_obj,
        char_code as u8,
        out_1 as *mut i32,
        out_2 as *mut i32,
    )
}

/// Port of `DisplayGfx::SetFontParam` (slot 10, 0x523710).
///
/// Per the original's register shuffle: `p3` = input string, `p4` =
/// output total advance, `p5` = output font max width.
pub unsafe extern "thiscall" fn set_font_param(
    this: *mut DisplayGfx,
    font_id: i32,
    p3: u32,
    p4: u32,
    p5: u32,
) -> u32 {
    if !is_valid_font_id(font_id) {
        return 0;
    }
    let font_obj = (*this).base.font_table[font_id as usize] as *const Font;
    if font_obj.is_null() {
        return 0;
    }
    font_set_param_impl(font_obj, p3 as *const u8, p4 as *mut i32, p5 as *mut i32);
    1
}

/// Port of `DisplayGfx::SetFontPalette` (slot 36, 0x523690).
///
/// Despite the name this is the entry point for `font_set_palette_impl`,
/// which extends the digital font with derived `'.'` and `';'` glyphs.
/// The original has no bounds or null check on `font_index`; we mirror that.
pub unsafe extern "thiscall" fn set_font_palette(
    this: *mut DisplayGfx,
    font_index: u32,
    palette_value: u32,
) {
    let font_obj = (*this).base.font_table[font_index as usize] as *mut Font;
    let _ = font_set_palette_impl(font_obj, palette_value);
}

/// Pure-Rust replacement for `FUN_004FA490` plus the 9 unrolled blit
/// helpers at `0x4FA1E0..0x4FA470` (dispatch table at `0x6A9594`).
///
/// The original splits a glyph into `min(remaining_width, 8)`-pixel
/// chunks and dispatches to one of 9 hand-unrolled helpers. We collapse
/// all of that into one nested loop. No transparency: every source byte
/// (palette index) is copied verbatim, including 0.
#[inline]
pub unsafe fn font_blit_glyph(
    dst: *mut u8,
    dst_stride: i32,
    src: *const u8,
    src_stride: i32,
    width: i32,
    height: i32,
) {
    for row in 0..height {
        let dst_row = dst.offset((row * dst_stride) as isize);
        let src_row = src.offset((row * src_stride) as isize);
        core::ptr::copy_nonoverlapping(src_row, dst_row, width as usize);
    }
}

/// Pure-Rust port of `Font__DrawText` (0x4FA4E0).
///
/// Rasterizes `msg` into `bitmap`. Glyph rows are copied verbatim — the
/// "background" of each glyph carries whatever palette index the .fnt
/// file baked in, including 0.
///
/// Subtle behaviors that match the original exactly:
///
/// - `*out_width = font.width` is written unconditionally up front,
///   even on the early validation-failure paths.
/// - On success: returns the number of chars drawn (forward path) or
///   `-1` (right-aligned, full string drawn).
/// - On truncation: returns the index of the first un-drawn char.
/// - On validation failure (negative pen, vertical overflow): returns 0
///   but `*out_width` has already been written.
/// - `out_pen_x` is initialized to 0 and updated to the running advance.
///
/// **Glyph source row stride** is `glyph.width` for base-font glyphs but
/// `font.width` for extension glyphs (index `>= font._height2`, added by
/// `font_extend` / `font_set_palette_impl`). Extension glyphs live in a
/// separate uniform-stride buffer.
///
/// `font_id_high` is the sign-extended high half of slot 7's `font_id`
/// (the wrapper does `SAR EAX, 0x10`). Bit 1 selects right-aligned mode.
pub unsafe fn font_draw_text_impl(
    font_obj: *const Font,
    bitmap: *const BitGrid,
    pen_x: i32,
    pen_y: i32,
    msg: *const c_char,
    out_pen_x: *mut i32,
    out_width: *mut i32,
    font_id_high: i32,
) -> i32 {
    let msg = msg as *const u8;
    let font = &*font_obj;
    let font_width = font.width as i16 as i32;

    // Written unconditionally — even on validation failure (matches original).
    *out_width = font_width;

    let bm = &*bitmap;
    let bitmap_width = bm.width as i32;
    let bitmap_height = bm.height as i32;
    let stride = bm.row_stride as i32;

    if pen_x < 0 || pen_y < 0 || font_width + pen_y > bitmap_height {
        return 0;
    }

    // Pre-offset data pointer to (pen_x, pen_y) — glyph dst calc only
    // needs the per-glyph delta after this.
    let data_origin = bm.data.offset((pen_y * stride + pen_x) as isize);

    let height2 = font._height2 as i16 as i32;
    let char_to_glyph = font.char_to_glyph_idx;
    let glyph_table = font.glyph_table;
    let pixel_data = font.pixel_data;
    let width_div_5 = font.width_div_5 as i16 as i32;

    *out_pen_x = 0;

    if (font_id_high >> 1) & 1 != 0 {
        // Right-aligned path: walk msg right-to-left, advance leftward.
        let mut len: i32 = 0;
        while *msg.offset(len as isize) != 0 {
            len += 1;
        }
        let mut idx = len - 1;
        if idx < 0 {
            return idx;
        }

        loop {
            let ch = *msg.offset(idx as isize) as usize;
            let glyph_idx_1based = *char_to_glyph.add(ch);
            if glyph_idx_1based == 0 {
                *out_pen_x += width_div_5;
            } else {
                let glyph_idx = glyph_idx_1based as i32 - 1;
                let glyph = &*glyph_table.add(glyph_idx as usize);
                let glyph_width = glyph.width as i32;
                let glyph_height = glyph.height as i32;
                let cur_advance = *out_pen_x;

                // Right-align fit check: leftmost edge must be ≥ 0.
                let leftmost = pen_x - cur_advance - glyph_width;
                if leftmost < 0 {
                    return idx;
                }

                let src_stride = if glyph_idx >= height2 {
                    font_width
                } else {
                    glyph_width
                };
                let dst = data_origin
                    .offset(((glyph.start_y as i32) * stride - glyph_width - cur_advance) as isize);
                let src = pixel_data.add(glyph.pixel_offset as usize);
                font_blit_glyph(dst, stride, src, src_stride, glyph_width, glyph_height);

                *out_pen_x += glyph_width + 1;
            }
            idx -= 1;
            if idx < 0 {
                return idx;
            }
        }
    } else {
        // Forward path: walk msg left-to-right, advance rightward.
        if *msg == 0 {
            return 0;
        }
        let mut idx: i32 = 0;
        loop {
            let cur_advance = *out_pen_x;
            if cur_advance + pen_x >= bitmap_width {
                return idx;
            }

            let ch = *msg.offset(idx as isize) as usize;
            let glyph_idx_1based = *char_to_glyph.add(ch);
            if glyph_idx_1based == 0 {
                *out_pen_x = cur_advance + width_div_5;
            } else {
                let glyph_idx = glyph_idx_1based as i32 - 1;
                let glyph = &*glyph_table.add(glyph_idx as usize);
                let glyph_width = glyph.width as i32;
                let glyph_height = glyph.height as i32;

                // Glyph must end at least 2 px before the bitmap edge.
                if glyph_width + cur_advance + pen_x + 2 > bitmap_width {
                    return idx;
                }

                let src_stride = if glyph_idx >= height2 {
                    font_width
                } else {
                    glyph_width
                };
                let dst =
                    data_origin.offset(((glyph.start_y as i32) * stride + cur_advance) as isize);
                let src = pixel_data.add(glyph.pixel_offset as usize);
                font_blit_glyph(dst, stride, src, src_stride, glyph_width, glyph_height);

                *out_pen_x += glyph_width + 1;
            }
            idx += 1;
            if *msg.offset(idx as isize) == 0 {
                return idx;
            }
        }
    }
}

/// Port of `DisplayGfx::DrawTextOnBitmap` (slot 7, 0x5236B0).
///
/// `font_id` low half = 1-based slot index (1..=31), high half = flags
/// (sign-extended via `SAR EAX, 0x10`). On validation failure (bad slot
/// or null entry) the original returns 0 *without* writing `*out_width`;
/// we mirror that.
pub unsafe extern "thiscall" fn draw_text_on_bitmap(
    this: *mut DisplayGfx,
    font_id: i32,
    bitmap: *mut BitGrid,
    pen_x: i32,
    pen_y: i32,
    msg: *const c_char,
    out_pen_x: *mut i32,
    out_width: *mut i32,
) -> i32 {
    let font_id_low = font_id & 0xFFFF;
    if !is_valid_font_id(font_id_low) {
        return 0;
    }
    let font_obj = (*this).base.font_table[font_id_low as usize] as *const Font;
    if font_obj.is_null() {
        return 0;
    }
    let font_id_high = (font_id as i32) >> 16;
    font_draw_text_impl(
        font_obj,
        bitmap as *const BitGrid,
        pen_x,
        pen_y,
        msg,
        out_pen_x,
        out_width,
        font_id_high,
    )
}

// =============================================================================
// DrawTiledBitmap (slot 11) and its leaf primitives
// =============================================================================

use crate::render::display::context::Surface;
use crate::render::sprite::sprite::CBitmap;
use crate::wa_alloc::wa_malloc;

/// Source descriptor passed to `DisplayGfx::DrawTiledBitmap` (slot 11) as
/// the third stack arg. Field offsets verified from the slot 11 disasm
/// (`0x56B8C0`): only `+0x08`, `+0x10`, `+0x14`, `+0x18` are read.
#[repr(C)]
pub struct TiledBitmapSource {
    /// 0x00..0x07: unknown header bytes
    pub _unknown_00: [u8; 8],
    /// 0x08: source pixel data base pointer. Used as the start of the
    /// blit source; per-strip offset is `data + current_y * row_stride`.
    pub data: *const u8,
    /// 0x0C: unknown
    pub _unknown_0c: u32,
    /// 0x10: source row stride in bytes (passed as `src_stride` to the
    /// `0x40`-bpp blit primitive; for the 8bpp path the inner loop ignores
    /// it and advances 8 bytes per row internally).
    pub row_stride: i32,
    /// 0x14: bpp dispatch value. `8` selects the 8-byte-pattern replicator
    /// ([`blit_64byte_row_pattern`]); `0x40` selects the transparent CLUT
    /// blit ([`blit_color_table_forward`]); any other value skips the blit.
    pub bpp: u32,
    /// 0x18: total source height in rows. The cache is allocated as
    /// `ceil(source_height / 0x400)` strips of `min(remaining, 0x400)` rows.
    pub source_height: i32,
}

const _: () = assert!(core::mem::offset_of!(TiledBitmapSource, data) == 0x08);
const _: () = assert!(core::mem::offset_of!(TiledBitmapSource, row_stride) == 0x10);
const _: () = assert!(core::mem::offset_of!(TiledBitmapSource, bpp) == 0x14);
const _: () = assert!(core::mem::offset_of!(TiledBitmapSource, source_height) == 0x18);

/// Pure-Rust port of `FUN_005B2A5E` — 8bpp 64-byte row replicator.
///
/// For each of `row_count` rows: read 8 bytes from `src`, write them 8
/// times consecutively to `dst` (= 64 bytes per row), advance `dst` by
/// `dst_stride` and `src` by exactly 8 bytes. The original is asm
/// hand-unrolled to 16 dword writes per row.
///
/// Only caller (verified via xrefs) is `DrawTiledBitmap` slot 11's 8bpp
/// populate phase.
unsafe fn blit_64byte_row_pattern(
    mut dst: *mut u8,
    dst_stride: i32,
    mut src: *const u8,
    row_count: i32,
) {
    let mut remaining = row_count;
    while remaining > 0 {
        let pattern_lo = (src as *const u32).read_unaligned();
        let pattern_hi = (src.add(4) as *const u32).read_unaligned();

        // Replicate (lo, hi) 8 times = 16 dwords = 64 bytes per row.
        let dst_dw = dst as *mut u32;
        let mut i = 0;
        while i < 8 {
            dst_dw.add(i * 2).write_unaligned(pattern_lo);
            dst_dw.add(i * 2 + 1).write_unaligned(pattern_hi);
            i += 1;
        }

        src = src.add(8);
        dst = dst.offset(dst_stride as isize);
        remaining -= 1;
    }
}

/// Pure-Rust port of `BlitColorTable_Forward` (0x5B2B5D) — transparent
/// byte-level blit (skip source bytes that are 0).
///
/// In our build only `DrawTiledBitmap` (slot 11) reaches this — the
/// other WA caller `BitGrid::BlitSpriteRect` (0x4F6C93) is replaced by
/// our `blit_sprite_rect` in `sprite_blit.rs`.
unsafe fn blit_color_table_forward(
    mut dst: *mut u8,
    dst_stride: i32,
    mut src: *const u8,
    src_stride: i32,
    width: i32,
    height: i32,
) {
    let mut rows_left = height;
    while rows_left > 0 {
        let mut col = 0;
        while col < width as isize {
            let s = *src.offset(col);
            if s != 0 {
                *dst.offset(col) = s;
            }
            col += 1;
        }
        dst = dst.offset(dst_stride as isize);
        src = src.offset(src_stride as isize);
        rows_left -= 1;
    }
}

/// Pure-Rust port of `FUN_00403C60` — the `CBitmap` blit-via-wrapper.
///
/// Lazy-allocs `cbm.surface` via `alloc_surface` (slot 22) on first
/// call, then dispatches the blit through `draw_landscape` (slot 23).
/// Note: `alloc_surface` returns its result in EAX, not via the
/// `FastcallResult` buffer — see `feedback_alloc_surface_return.md`.
unsafe fn cbitmap_blit_via_wrapper(
    cbm: *mut CBitmap,
    dst_x: i32,
    dst_y: i32,
    src_x: i32,
    src_y: i32,
    width: i32,
    height: i32,
    flags: u32,
) {
    let wrapper = *(rb(va::G_RENDER_CONTEXT) as *const *mut RenderContext);

    if (*cbm).surface.is_null() {
        let mut buf = FastcallResult::default();
        let ret = RenderContext::alloc_surface_raw(wrapper, &mut buf);
        (*cbm).surface = ret as *mut Surface;
    }

    let mut buf = FastcallResult::default();
    RenderContext::draw_landscape_raw(
        wrapper,
        &mut buf,
        (*cbm).surface as *mut u8,
        dst_x,
        dst_y,
        src_x,
        src_y,
        width,
        height,
        flags,
    );
}

/// Pure-Rust port of `DisplayGfx::BlitBitmapClipped` (0x56A700).
///
/// Used by slots 11 (`DrawTiledBitmap`), 22 (`DrawTiledTerrain`), and
/// the bitmap-sprite branch of slot 19 (`BlitSprite`).
pub unsafe fn blit_bitmap_clipped_native(
    this: *mut DisplayGfx,
    dst_x: i32,
    dst_y: i32,
    width: i32,
    height: i32,
    surface: *mut CBitmap,
    flags: u32,
) {
    let base = &(*this).base;
    let cx1 = base.clip_x1;
    let cy1 = base.clip_y1;
    let cx2 = base.clip_x2;
    let cy2 = base.clip_y2;

    let dst_x2 = dst_x + width;
    let dst_y2 = dst_y + height;

    if dst_x >= cx2 || dst_x2 <= cx1 || dst_y >= cy2 || dst_y2 <= cy1 {
        return;
    }

    let new_left = cx1.max(dst_x);
    let new_right = cx2.min(dst_x2);
    let new_top = cy1.max(dst_y);
    let new_bottom = cy2.min(dst_y2);

    // Degenerate clipped rect — matches the original's
    // `local_28 != iVar2 && local_24 != iVar1` check.
    if new_left == new_right || new_top == new_bottom {
        return;
    }

    flush_render_lock(this);

    cbitmap_blit_via_wrapper(
        surface,
        new_left,
        new_top,
        new_left - dst_x,
        new_top - dst_y,
        new_right - new_left,
        new_bottom - new_top,
        flags | 1,
    );
}

/// Pure-Rust port of `DisplayGfx::BlitBitmapTiled` (0x56A7D0; usercall
/// `EAX=initial_x, EDI=tile_width`).
///
/// Tiles `surface` horizontally across `[clip_x1, clip_x2)`. Used by
/// slot 19's bitmap-sprite branch when the tiled mode bit is set.
pub unsafe fn blit_bitmap_tiled_native(
    this: *mut DisplayGfx,
    initial_x: i32,
    tile_width: i32,
    dst_y: i32,
    height: i32,
    surface: *mut CBitmap,
) {
    let clip_x1 = (*this).base.clip_x1;
    let clip_x2 = (*this).base.clip_x2;

    // Two-loop walk to the largest `x ≤ clip_x1` in the sequence
    // {initial_x ± k*tile_width}, matching 0x56A7DD..0x56A7F4.
    let mut x = initial_x;
    while x < clip_x1 {
        x += tile_width;
    }
    while x > clip_x1 {
        x -= tile_width;
    }

    while x < clip_x2 {
        blit_bitmap_clipped_native(this, x, dst_y, tile_width, height, surface, 2);
        x += tile_width;
    }
}

/// Pure-Rust port of `DisplayGfx::GetBitmapSpriteInfo` (0x573C50;
/// usercall `EAX=bitmap_obj, EDX=palette_or_anim`).
///
/// `bitmap_obj.flags` interprets `palette_or_anim`:
/// - bit 0: 0 = signed clamp to `[0, 0xFFFF]`, 1 = use low 16 bits as-is
/// - bit 1: 0 = forward iter, 1 = ping-pong over `[0, frame_count)`
///
/// Returns a pointer to the selected `LayerSpriteFrame`'s embedded
/// `CBitmap` (the trailing 12 bytes). Used by slot 19's bitmap-sprite branch.
pub unsafe fn get_bitmap_sprite_info(
    bitmap_obj: *mut LayerSprite,
    palette_or_anim: u32,
    out_w: *mut i32,
    out_h: *mut i32,
    out_left: *mut i32,
    out_top: *mut i32,
    out_right: *mut i32,
    out_bottom: *mut i32,
) -> *mut CBitmap {
    let flags = (*bitmap_obj).flags as i32;

    let pal: i32 = if flags & 1 != 0 {
        (palette_or_anim & 0xFFFF) as i32
    } else {
        let p = palette_or_anim as i32;
        p.max(0).min(0xFFFF)
    };

    let frame_count = (*bitmap_obj).frame_count as i16 as i32;
    let frame_idx = if flags & 2 != 0 {
        // Ping-pong: scaled = ((2*frame_count - 1) * pal) >> 16, fold
        // back to `(2*frame_count - scaled) - 1` when past the midpoint.
        let scaled = ((frame_count * 2 - 1) * pal) >> 16;
        if scaled >= frame_count {
            (frame_count * 2 - scaled) - 1
        } else {
            scaled
        }
    } else {
        (frame_count * pal) >> 16
    };

    let frame = (*bitmap_obj).frame_array.offset(frame_idx as isize);

    *out_left = (*frame).start_x as i32;
    *out_top = (*frame).start_y as i32;
    *out_right = (*frame).end_x as i32;
    *out_bottom = (*frame).end_y as i32;

    *out_w = (*bitmap_obj).cell_width as i32;
    *out_h = (*bitmap_obj).cell_height as i32;

    LayerSpriteFrame::bitmap_ptr(frame)
}

/// Pure-Rust port of `DisplayGfx::DrawTiledBitmap` (vtable slot 11,
/// `0x56B8C0`).
///
/// Three-phase tile-cached landscape blit. See the docstring on
/// [`DisplayVtable::draw_tiled_bitmap`] for the high-level semantics.
///
/// **Tile-cache vector pre-reservation.** The original allocates and
/// `vector::push_back`s each strip's `CBitmap*` into `bitmap_vec`
/// (`+0x3580`) one at a time, growing the vector as needed. We sidestep
/// porting `std::vector::push_back` (`FUN_00402e90`) by computing the
/// final entry count up front (`ceil(source_height / 0x400)`), allocating
/// the entire backing buffer in one shot, then writing entries directly
/// and bumping `bitmap_end`. This changes WA's heap allocation pattern
/// slightly (one bigger allocation vs many growing reallocs) but is
/// behaviorally equivalent: each `DisplayGfx` only ever populates the
/// vector once.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`. `source` must be a valid
/// `*const TiledBitmapSource` whose fields describe the source landscape
/// data. `g_RenderContext` must be initialized.
pub unsafe fn draw_tiled_bitmap_impl(
    this: *mut DisplayGfx,
    dest_x: i32,
    dest_y: i32,
    source: *const TiledBitmapSource,
) {
    let total_height = (*source).source_height;
    let row_stride = (*source).row_stride;
    let bpp = (*source).bpp;
    let source_data = (*source).data;

    // -------------------------------------------------------------------
    // Phase 1 — Allocate (only if bitmap_vec is empty)
    // -------------------------------------------------------------------
    let vec_empty = (*this).bitmap_ptr.is_null()
        || ((*this).bitmap_end as usize - (*this).bitmap_ptr as usize) >> 2 == 0;

    if vec_empty {
        if total_height > 0 {
            // Pre-reserve in one allocation; see the docstring above.
            let max_tiles = ((total_height + 0x3FF) >> 10) as usize;
            let vec_buf = wa_malloc((max_tiles * core::mem::size_of::<*mut CBitmap>()) as u32)
                as *mut *mut CBitmap;
            if vec_buf.is_null() {
                return;
            }
            (*this).bitmap_ptr = vec_buf;
            (*this).bitmap_end = vec_buf;
            (*this).bitmap_capacity = vec_buf.add(max_tiles);

            let cbitmap_vt = rb(va::CBITMAP_VTABLE_MAYBE) as *const core::ffi::c_void;
            let wrapper = *(rb(va::G_RENDER_CONTEXT) as *const *mut RenderContext);

            let mut accum = 0i32;
            let mut remaining = total_height;
            while accum < total_height {
                // The original doesn't null-check the malloc; mirror that.
                let cbm = wa_malloc_struct_zeroed::<CBitmap>();
                if !cbm.is_null() {
                    (*cbm).vtable = cbitmap_vt;
                    (*cbm).surface = core::ptr::null_mut();
                    (*cbm)._pad = 0;
                }

                let strip_h = remaining.min(0x400);

                if (*cbm).surface.is_null() {
                    let mut buf = FastcallResult::default();
                    let s = RenderContext::alloc_surface_raw(wrapper, &mut buf);
                    (*cbm).surface = s as *mut Surface;
                }

                // Init at 0x40 × strip_h × 8bpp; retry with 4bpp on failure.
                let mut init_buf = FastcallResult::default();
                Surface::init_surface_raw((*cbm).surface, &mut init_buf, 0x40, strip_h, 8);

                if init_buf.value != 0 {
                    if (*cbm).surface.is_null() {
                        let mut buf = FastcallResult::default();
                        let s = RenderContext::alloc_surface_raw(wrapper, &mut buf);
                        (*cbm).surface = s as *mut Surface;
                    }
                    let mut init_buf2 = FastcallResult::default();
                    Surface::init_surface_raw((*cbm).surface, &mut init_buf2, 0x40, strip_h, 4);
                    if init_buf2.value != 0 {
                        return;
                    }
                }

                *(*this).bitmap_end = cbm;
                (*this).bitmap_end = (*this).bitmap_end.add(1);

                accum += 0x400;
                remaining -= 0x400;
            }
        }
        (*this).tile_cache_populated = 0;
    }

    // -------------------------------------------------------------------
    // Phase 2 — Populate
    // -------------------------------------------------------------------
    if (*this).tile_cache_populated == 0 {
        let mut tile_idx: usize = 0;
        let mut current_y: i32 = 0;
        loop {
            let vec_size = if (*this).bitmap_ptr.is_null() {
                0
            } else {
                ((*this).bitmap_end as usize - (*this).bitmap_ptr as usize) >> 2
            };
            if tile_idx >= vec_size {
                break;
            }

            let strip_end = total_height.min(current_y + 0x400);
            let strip_h = strip_end - current_y;

            let cbm = *(*this).bitmap_ptr.add(tile_idx);

            // Paranoid lazy-alloc — should already be non-null from
            // phase 1, but the original repeats the check.
            if (*cbm).surface.is_null() {
                let wrapper = *(rb(va::G_RENDER_CONTEXT) as *const *mut RenderContext);
                let mut buf = FastcallResult::default();
                let s = RenderContext::alloc_surface_raw(wrapper, &mut buf);
                (*cbm).surface = s as *mut Surface;
            }

            let mut surf_data: *mut u8 = core::ptr::null_mut();
            let mut surf_stride: i32 = 0;
            {
                let mut buf = FastcallResult::default();
                Surface::lock_surface_raw(
                    (*cbm).surface,
                    &mut buf,
                    &mut surf_data,
                    &mut surf_stride,
                );
            }

            let src_row = source_data.offset((current_y as isize) * (row_stride as isize));

            if bpp == 8 {
                blit_64byte_row_pattern(surf_data, surf_stride, src_row, strip_h);
            } else if bpp == 0x40 {
                blit_color_table_forward(
                    surf_data,
                    surf_stride,
                    src_row,
                    row_stride,
                    0x40,
                    strip_h,
                );
            }
            // (other bpp values: no blit, just unlock — matches original)

            {
                let mut buf = FastcallResult::default();
                Surface::unlock_surface_raw((*cbm).surface, &mut buf, surf_data);
            }

            tile_idx += 1;
            current_y += 0x400;
        }
        (*this).tile_cache_populated = 1;
    }

    // -------------------------------------------------------------------
    // Phase 3 — Display
    // -------------------------------------------------------------------
    // Snap dest_x to a 0x40-aligned column ≤ dest_x: result is in
    // [-0x3f, 0], so stepping by 0x40 covers the visible area starting
    // off the left edge. Reproduces the original's
    // `AND EAX, 0x8000003f` + sign-extend dance for signed mod 0x40.
    let col_x: i32 = {
        let dest_x_u = dest_x as u32;
        let masked = dest_x_u & 0x8000_003f;
        let mut v = if (masked as i32) < 0 {
            (((masked.wrapping_sub(1)) | 0xffff_ffc0).wrapping_add(1)) as i32
        } else {
            masked as i32
        };
        if v > 0 {
            v -= 0x40;
        }
        v
    };

    // First/last visible Y-tile indices, using the original's signed
    // `(v + ((v >> 31) & 0x3FF)) >> 10` rounding-toward-zero idiom.
    let camera_y = (*this).camera_y;
    let neg = -(camera_y + dest_y);
    let display_height = (*this).base.display_height as i32;
    let first_v = neg + 0x20000;
    let last_v = display_height + neg + 0x20000;

    let mut y_first = (((first_v + ((first_v >> 31) & 0x3FF)) >> 10) - 0x80) as i32;
    let mut y_last = (((last_v + ((last_v >> 31) & 0x3FF)) >> 10) - 0x80) as i32;

    if y_first < 0 {
        y_first = 0;
    }

    let vec_size = if (*this).bitmap_ptr.is_null() {
        0i32
    } else {
        (((*this).bitmap_end as usize - (*this).bitmap_ptr as usize) >> 2) as i32
    };
    let max_idx = vec_size - 1;
    if max_idx <= y_last {
        y_last = vec_size - 1;
    }

    if y_first > y_last {
        return;
    }

    let display_width = (*this).base.display_width as i32;
    let mut tile_idx_y = y_first;
    let mut current_strip_y = y_first << 10;

    while tile_idx_y <= y_last {
        let strip_end = total_height.min(current_strip_y + 0x400);
        let strip_h = strip_end - current_strip_y;

        if col_x < display_width {
            let cbm = *(*this).bitmap_ptr.add(tile_idx_y as usize);
            let dst_y = camera_y + current_strip_y + dest_y;
            let mut x = col_x;
            while x < display_width {
                blit_bitmap_clipped_native(this, x, dst_y, 0x40, strip_h, cbm, 0);
                x += 0x40;
            }
        }

        tile_idx_y += 1;
        current_strip_y += 0x400;
    }
}

/// Thiscall entry point for `DisplayGfx::DrawTiledBitmap` (slot 11).
pub unsafe extern "thiscall" fn draw_tiled_bitmap(
    this: *mut DisplayGfx,
    dest_x: i32,
    dest_y: i32,
    source: *const TiledBitmapSource,
) {
    draw_tiled_bitmap_impl(this, dest_x, dest_y, source);
}
