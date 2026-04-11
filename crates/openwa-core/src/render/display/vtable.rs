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
    /// destructor (0x569CE0, RET 0x4)
    #[slot(0)]
    pub destructor: fn(this: *mut DisplayGfx, flags: u8) -> *mut DisplayGfx,
    /// get display dimensions in pixels (0x56A460, RET 0x8)
    #[slot(1)]
    pub get_dimensions: fn(this: *mut DisplayGfx, out_w: *mut u32, out_h: *mut u32),
    /// set layer color (0x5231E0, RET 0x8)
    #[slot(4)]
    pub set_layer_color: fn(this: *mut DisplayGfx, layer: i32, color: i32),
    /// set active layer, returns layer context ptr (0x523270, RET 0x4)
    #[slot(5)]
    pub set_active_layer: fn(this: *mut DisplayGfx, layer: i32) -> *mut u8,
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
    /// draw tile-cached bitmap (0x56B8C0, RET 0xC) — `DisplayGfx__DrawTiledBitmap`.
    ///
    /// Three-phase tile-bitmap operation, run as a single composite call.
    /// Each invocation does some or all of:
    ///
    /// 1. **Allocate** (only if `this->bitmap_vec` is empty): walk the source
    ///    height in 0x400-row strips. For each strip, `wa_malloc(0xC)` a
    ///    `CBitmap` (vtable `0x643F64`, surface ptr at +4), lazily ask the
    ///    render context for a surface, init the surface as `0x40 × strip × 8bpp`
    ///    (retrying with 4bpp on failure), and `vector::push_back` the bitmap
    ///    into `DisplayGfx + 0x3580`. Sets `DisplayGfx[+0x358C] = 0`.
    /// 2. **Populate** (only if `DisplayGfx[+0x358C] == 0`): for each tile,
    ///    lock its surface, blit the corresponding source strip via
    ///    `FUN_005B2A5E` (8bpp) or `BlitColorTable_Forward` (CLUT, `bpp == 0x40`),
    ///    unlock. Sets `DisplayGfx[+0x358C] = 1`.
    /// 3. **Display**: `dest_x` is masked to a 0x40-aligned X coord; the
    ///    visible Y range is computed from `camera_y + dest_y` against
    ///    `display_height`, clamped to the available tile range. For each
    ///    visible tile × each `0x40`-wide destination column, calls
    ///    `DisplayGfx__BlitBitmapClipped`.
    ///
    /// `source` is a [`TiledBitmapSource`] descriptor — see that struct
    /// for the field layout.
    ///
    /// **Reachable at runtime** via `RenderDrawingQueue` case 0xD, fed by
    /// `RQ_EnqueueTiledBitmap` (0x541D60). The only known producer is
    /// `CTaskLand::RenderLandscape`.
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
    /// Fixed-point input; internally `>> 16` to pixel integers stored at +0x3560/+0x3564.
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
    ///
    /// **Not a "load" function** despite the historic mis-name. Called every
    /// frame from `BlitSprite` (slot 19) to resolve a sprite ID + animation
    /// value into a renderable frame: clamps the animation value, looks up
    /// the matching frame entry in the sprite's frame table, lazily
    /// decompresses the frame's surface via `FUN_004FA950`, and returns
    /// the surface pointer plus the frame's bounding box.
    ///
    /// Dispatches via the two arrays at `DisplayGfx + 0x1008` (Sprite*)
    /// and `DisplayGfx + 0x2008` (SpriteBank*) — both lookups go through
    /// the same outputs:
    ///
    /// | Output                         | Meaning                                          |
    /// |--------------------------------|--------------------------------------------------|
    /// | return value                   | `*mut DisplayBitGrid` — decompressed frame surface |
    /// | `out_w`, `out_h`               | full sprite cell width / height (for centering)  |
    /// | `out_left`/`top`/`right`/`bot` | frame bounding box within the cell               |
    /// | `out_anim_frac`                | sub-frame interpolation value (Fixed16) or 0     |
    ///
    /// `Sprite*` path → `Sprite__GetFrameForBlit` (FUN_004FAD30, ESI=sprite).
    /// `SpriteBank*` path → `SpriteBank__GetFrameForBlit` (FUN_004F9710,
    /// ESI=bank). Both inner helpers use ESI for the receiver in a complex
    /// usercall convention, which is why this slot has not been ported yet —
    /// it would require porting the FrameCache and the surface decompression
    /// helper (`FUN_004FA950` / `FUN_005B29E0`) first.
    ///
    /// `BlitSprite` calls this directly via `vtable[33]` today; see
    /// `replacements/render.rs::blit_sprite`.
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
        char_map: *const u8,
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
use crate::render::sprite::{Sprite, SpriteBank, SpriteVtable};

/// Port of DisplayGfx::GetDimensions (vtable slot 1, 0x56A460).
///
/// Reads display_width/display_height from DisplayBase and writes them
/// to the output pointers.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
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

/// Port of DisplayGfx::FlushRender (vtable slot 26, 0x56A580).
///
/// If the render lock is held, clears it (the original also calls
/// RenderContext::unlock_surface_write, but that's a no-op that
/// writes a success code to a discarded result buffer).
///
/// Then calls RenderContext::get_renderer_surface (slot 13),
/// which dispatches to the renderer backend's Flip.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
/// `g_RenderContext` (0x79D6D4) must be initialized.
pub unsafe extern "thiscall" fn flush_render(this: *mut DisplayGfx) {
    let wrapper = *(rb(va::G_RENDER_CONTEXT) as *const *mut RenderContext);

    if (*this).render_lock != 0 {
        // Original calls wrapper->vtable[18] (unlock_surface_write) here,
        // but that function ignores its data parameter and just writes a
        // success code to a result buffer that FlushRender never reads.
        (*this).render_lock = 0;
    }

    // get_renderer_surface → renderer Flip
    let mut buf = FastcallResult::default();
    RenderContext::get_renderer_surface_raw(wrapper, &mut buf);
}

/// Port of DisplayGfx::SetCameraOffset (vtable slot 27, 0x56CC40).
///
/// Converts Fixed-point camera coordinates to pixel integers and stores them.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
pub unsafe extern "thiscall" fn set_camera_offset(this: *mut DisplayGfx, x: Fixed, y: Fixed) {
    (*this).camera_x = x.to_int();
    (*this).camera_y = y.to_int();
}

/// Port of DisplayGfx::SetClipRect (vtable slot 28, 0x56CC60).
///
/// Converts Fixed-point clip rectangle to pixel integers, clamps to display
/// dimensions, stores in DisplayBase, and mirrors to the layer_0 object.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
/// `layer_0` (at +0x3D9C) must be initialized.
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

/// Port of the render-lock flush helper at 0x56A330.
///
/// If the render lock is held, releases it by calling
/// RenderContext::unlock_surface_write (slot 18), then clears the flag.
/// Called by drawing methods (fill_rect, draw_line, etc.) before rendering.
///
/// Original uses ESI = this (usercall).
///
/// # Safety
/// `gfx` must be a valid `*mut DisplayGfx` with `layer_0` initialized.
/// `g_RenderContext` must be initialized.
unsafe fn flush_render_lock(gfx: *mut DisplayGfx) {
    if (*gfx).render_lock != 0 {
        let wrapper = *(rb(va::G_RENDER_CONTEXT) as *const *mut RenderContext);
        let data = (*(*gfx).layer_0).data as u32;
        let mut buf = FastcallResult::default();
        RenderContext::unlock_surface_write_raw(wrapper, &mut buf, data);
        (*gfx).render_lock = 0;
    }
}

/// Port of the render-lock acquire helper at 0x56A370.
///
/// If the render lock is NOT held, queries RenderContext for framebuffer
/// dimensions (slot 3) and locks the surface for writing (slot 17), then
/// populates layer_0's BitGrid fields (data, stride, dimensions) from the
/// locked surface and copies DisplayBase's clip rect into layer_0.
///
/// Original uses ESI = this (usercall).
///
/// # Safety
/// `gfx` must be a valid `*mut DisplayGfx` with `layer_0` initialized.
/// `g_RenderContext` must be initialized.
pub unsafe fn acquire_render_lock(gfx: *mut DisplayGfx) {
    if (*gfx).render_lock != 0 {
        return; // already locked
    }

    let wrapper = *(rb(va::G_RENDER_CONTEXT) as *const *mut RenderContext);
    let mut buf = FastcallResult::default();

    // Get framebuffer dimensions (slot 3).
    // The wrapper writes width and height to the output buffer.
    let mut dims: [u32; 2] = [0; 2];
    RenderContext::get_framebuffer_dims_raw(wrapper, &mut buf, dims.as_mut_ptr());
    let fb_width = dims[0];
    let fb_height = dims[1];

    // Lock surface for writing (slot 17).
    // The wrapper writes framebuffer pointer and stride through the params.
    let mut data_ptr: u32 = 0;
    let mut stride: u32 = 0;
    RenderContext::lock_surface_write_raw(wrapper, &mut buf, &mut data_ptr, &mut stride);

    // Populate layer_0 from the locked surface
    let layer = (*gfx).layer_0;
    if (*layer).external_buffer != 0 {
        (*layer).width = fb_width;
        (*layer).height = fb_height;
        (*layer).data = data_ptr as *mut u8;
        (*layer).row_stride = stride;
        (*layer).clip_left = 0;
        (*layer).clip_top = 0;
        (*layer).clip_right = fb_width;
        (*layer).clip_bottom = fb_height;
    }

    // Copy DisplayBase clip rect to layer_0, clamped to layer dimensions
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

/// Port of DisplayGfx::FillRect (vtable slot 18, 0x56B810).
///
/// Fills a rectangle with a solid color. Pixel-integer coordinates are
/// offset by the camera position and clipped to the clip rect before
/// dispatching to RenderContext::fill_rect (slot 19).
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
/// `g_RenderContext` must be initialized.
pub unsafe extern "thiscall" fn fill_rect(
    this: *mut DisplayGfx,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    color: u32,
) {
    let base = &(*this).base;

    // Apply camera offset
    let mut left = x1 + (*this).camera_x;
    let mut top = y1 + (*this).camera_y;
    let mut right = x2 + (*this).camera_x;
    let mut bottom = y2 + (*this).camera_y;

    // Early-out: no intersection with clip rect
    if right <= base.clip_x1
        || bottom <= base.clip_y1
        || left >= base.clip_x2
        || top >= base.clip_y2
    {
        return;
    }

    // Clamp to clip rect
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

    // RenderContext::fill_rect takes (x, y, width, height, color)
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

/// Port of DisplayGfx::DrawOutlinedPixel (vtable slot 17, 0x56BFD0).
///
/// Draws a center pixel in `color_fg` with 4 cardinal neighbor pixels in
/// `color_bg` (if `color_bg != 0`). Pixel-integer coordinates with camera offset.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
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

/// Port of DisplayGfx::DrawCrosshair (vtable slot 16, 0x56BE80).
///
/// Draws a 2x2 foreground block at (cx, cy)–(cx+1, cy+1) with an 8-pixel
/// outline in `color_bg`. Pixel-integer coordinates with camera offset.
///
/// ```text
///     bg bg
///  bg FG FG bg
///  bg FG FG bg
///     bg bg
/// ```
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
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

    // Background outline (8 pixels)
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx, cy - 1, bg);
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx + 1, cy - 1, bg);
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx - 1, cy, bg);
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx + 2, cy, bg);
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx - 1, cy + 1, bg);
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx + 2, cy + 1, bg);
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx, cy + 2, bg);
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx + 1, cy + 2, bg);

    // Foreground 2x2 block (4 pixels)
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx, cy, fg);
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx + 1, cy, fg);
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx, cy + 1, fg);
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx + 1, cy + 1, fg);
}

/// Wrapper that implements `PixelWriter` for a raw `*mut DisplayBitGrid`.
///
/// Dispatches `put_pixel_clipped` through the vtable, reads clip rect from
/// the BitGrid's clip fields.
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

/// Port of DisplayGfx::DrawLine (vtable slot 13, 0x56BDB0).
///
/// Draws a two-color thick line. Fixed-point coordinates with camera offset
/// applied as `camera * 0x10000 + coord`. Pure Rust implementation.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
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

/// Port of DisplayGfx::DrawLineClipped (vtable slot 14, 0x56BD50).
///
/// Draws a single-color clipped line. Fixed-point coordinates with camera
/// offset. Pure Rust implementation.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
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

/// Port of DisplayGfx::DrawPolyline (vtable slot 12, 0x56BCC0).
///
/// Transforms point array by camera offset, clips, and fills the polygon.
/// Points are Fixed-point (x, y) pairs. Pixel-integer camera is applied as
/// `camera * 0x10000 + coord`.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
pub unsafe extern "thiscall" fn draw_polyline(
    this: *mut DisplayGfx,
    points: *mut i32,
    count: i32,
    color: u32,
) {
    let cam_x = Fixed::from_int((*this).camera_x);
    let cam_y = Fixed::from_int((*this).camera_y);

    // Build vertex array with camera offset on the stack
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

/// Port of DisplayGfx::IsSpriteLoaded (vtable slot 32, 0x56A480).
///
/// Returns 1 if the sprite ID is loaded in any of the three sprite arrays
/// (DisplayBase sprite_ptrs/sprite_banks, DisplayGfx sprite_table).
/// ID must be in range [1, 0x400).
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
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

/// Check if a sprite ID is in the valid range [1, 0x3FF].
#[inline]
fn is_valid_sprite_id(id: i32) -> bool {
    (1..=0x3FF).contains(&id)
}

/// Port of DisplayGfx::GetSpriteInfo (vtable slot 6, 0x523500).
///
/// Looks up sprite metadata for `layer` (valid: 1..=0x3FF). Checks
/// `sprite_ptrs` (Sprite* pointers) first, then `sprite_banks`
/// (indexed sprite containers). On success, writes data, flags, and
/// width to the output pointers and returns a pointer to the static
/// string "sprite" (0x664170). Returns 0 on failure.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
/// Output pointers must be valid for writing.
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

/// Address of the static "sprite" string in WA.exe .rdata.
/// Used as a type-tag return value by sprite info functions.
const SPRITE_STRING: u32 = va::STR_SPRITE;

/// Extract sprite info from a Sprite object — port of Sprite__GetInfo (0x4FAEC0).
///
/// Usercall: EAX=Sprite*, ESI=out_data, ECX=out_width, stack=out_flags.
/// Reads frame_meta_ptr (validity check), packed _unknown_08/fps as data,
/// max_frames as width (doubled-minus-one if flags & 2), and flags & 1.
unsafe fn sprite_info_from_sprite(
    sprite: *const Sprite,
    out_data: *mut u32,
    out_flags: *mut u32,
    out_width: *mut u32,
) -> u32 {
    let s = &*sprite;

    // Validity check: frame_meta_ptr must be non-null
    if s.frame_meta_ptr.is_null() {
        return 0;
    }

    // out_data = packed DWORD: _unknown_08 (u16) | fps (u16) << 16
    *out_data = (s._unknown_08 as u32) | ((s.fps as u32) << 16);

    // Width from max_frames, doubled-minus-one if flags bit 1 set
    let mut width = s.max_frames as u32;
    if s.flags & 2 != 0 {
        width = width * 2 - 1;
    }
    *out_width = width;

    *out_flags = (s.flags & 1) as u32;

    crate::rebase::rb(SPRITE_STRING)
}

/// Extract sprite info from a SpriteBank — port of SpriteBank__GetInfo (0x4F98C0).
///
/// Usercall: EAX=layer, ECX=SpriteBank*, ESI=out_width, stack=out_data+out_flags.
/// Uses the bank's index table to map the layer ID to a frame entry, then
/// reads width/flags/data from the SpriteBankFrame.
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

    if frame.width & 0x8000 != 0 {
        *out_width = 1;
    } else if frame.flags & 2 != 0 {
        *out_width = (frame.width as u32) * 2 - 1;
    } else {
        *out_width = frame.width as u32;
    }

    crate::rebase::rb(SPRITE_STRING)
}

/// Port of DisplayGfx::DrawViaCallback (vtable slot 21, 0x56B7C0).
///
/// Acquires the render lock, applies camera offset to fixed-point coordinates,
/// then calls `obj->vtable[2](layer_0, pixel_x, pixel_y, p5, p6)`.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
/// `obj` must point to a valid object with a vtable where slot 2 is a
/// drawing callback.
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

    // Call obj->vtable[2](obj, layer_0, pixel_x, pixel_y, p5, p6)
    // vtable[2] is at offset 8 in the vtable
    let vtable = *(obj as *const *const u32);
    let callback: unsafe extern "thiscall" fn(*mut u8, *mut DisplayBitGrid, i32, i32, u32, u32) =
        core::mem::transmute(*vtable.add(2));
    callback(obj, layer_0, pixel_x, pixel_y, p5, p6);
}

/// Port of DisplayGfx::StreamData (vtable slot 22, 0x56C5A0).
///
/// Tiles bitmaps from a grid configuration in row-major order with camera offset.
/// The tile grid is defined by `tile_total_width/height` and `tile_col_width/row_height`
/// fields on DisplayGfx. Bitmaps come from the object at `tile_bitmap_sets[1]`,
/// whose field at +0x04 is a pointer array of bitmap pointers.
///
/// `x`/`y` are Fixed-point (>> 16 for pixels). `count` limits how many pixel-rows
/// are rendered. `flags` low 16 bits must be 1 (only supported mode); bit 19
/// controls a blit transparency flag.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
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

    // Only mode 1 is supported
    if (flags & 0xFFFF) != 1 {
        return;
    }

    // Get the bitmap tile set (mode 1 → tile_bitmap_sets[1] at offset 0x4DD8)
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

        // Clamp row height to remaining count
        let mut row_h = row_height;
        if count - y_offset < row_height {
            row_h = count - y_offset;
        }

        let mut x_offset = 0i32;
        while x_offset < total_width {
            // Clamp column width to remaining grid width
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

/// Port of DisplayGfx::DrawPixelStrip (vtable slot 15, 0x56BE10).
///
/// Draws `count + 1` pixels starting at (x, y), stepping by (dx, dy) each
/// iteration. All coordinates are Fixed-point. Camera applied as
/// `camera * 0x10000 + coord`.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
/// Port of DisplayGfx::DrawScaledSprite (vtable slot 20, 0x56B660).
///
/// Blits a source BitGrid to the display layer with camera offset and centering.
/// The source rectangle is `(src_x, src_y)` to `(src_w, src_h)` — width and
/// height are computed as `src_w - src_x` and `src_h - src_y`.
///
/// Dispatches to different blit modes based on flag bits:
/// - Default: normal blit (color table mode for transparency)
/// - 0x200000: additive blend via color_add_table LUT
/// - 0x4000000: color blend via color_blend_table LUT
/// - 0x8000000 / 0x10000000: stippled (checkerboard) blit
///
/// Bit 20 controls the blend mode: clear = ColorTable (transparency), set = Copy (opaque).
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
/// `sprite` must be a valid `*mut DisplayBitGrid`.
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

    // Signed division rounding toward zero: (n + (n >> 31)) >> 1
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

    // Camera offset + centering + fixed-point to pixel conversion
    let dst_x = (*this).camera_x - half_w + (x.0 >> 16);
    let dst_y = (*this).camera_y - half_h + (y.0 >> 16);

    // Blend mode from flag bit 20: clear = 1 (ColorTable), set = 0 (Copy)
    let blend_mode = (!(flags >> 20)) & 1;

    // Stippled modes (checkerboard blit)
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

    // Determine color table pointer from flags
    let color_table: *const u8 = if (flags & 0x200000) != 0 {
        // Additive: use color_add_table (offset 0x4DF4 in DisplayGfx)
        (*this).color_add_table.as_ptr()
    } else if (flags & 0x4000000) != 0 {
        // Color blend: use color_blend_table (offset 0x14DF4 in DisplayGfx)
        (*this).color_blend_table.as_ptr()
    } else {
        // Normal: no color table (transparency handled by blend mode 1)
        core::ptr::null()
    };

    // Early out if zero-size
    if width <= 0 || height <= 0 {
        return DrawScaledSpriteResult::Handled;
    }

    acquire_render_lock(this);

    let layer = (*this).layer_0;

    // Build flags for core blit: blend_mode in low 16 bits
    // The core blit interprets: 0 = Copy, 1 = ColorTable
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

/// Result of draw_scaled_sprite coordinate computation.
///
/// The actual blit call is performed by the DLL hook layer, since it needs
/// access to `blit_impl` which bridges DisplayBitGrid → PixelGrid.
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

/// Port of DisplayGfx::DrawPixelStrip (vtable slot 15, 0x56BE10).
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
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

/// Port of DisplayGfx::SetLayerColor (vtable slot 4, 0x5231E0).
///
/// Allocates a `PaletteContext` for the given layer (1-3) if one doesn't exist.
/// Finds `color` consecutive available entries in the slot table, claims them,
/// and initializes a PaletteContext with that palette index range.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
pub unsafe extern "thiscall" fn set_layer_color(this: *mut DisplayGfx, layer: i32, color: i32) {
    let Some(layer) = Layer::try_from_i32(layer) else {
        return;
    };

    // Only allocate if no context exists for this layer
    if !(*this).base.layer_contexts[layer.idx()].is_null() {
        return;
    }

    // Find `color` consecutive available (non-zero) entries in the slot table area.
    // The scan covers slot_table_guard (guard=0) + slot_table[0..255] + slot_table_sentinel (sentinel=-1).
    let start = palette_slot_alloc(&mut (*this).base, color);

    // Allocate PaletteContext (0x72C total, zero first 0x70C)
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
/// Scans the slot table area (this+0x312C) for `count` consecutive entries
/// with value > 0 (available). Zeros the found entries to mark them as claimed.
/// Returns the start index, or -1 if a negative sentinel is hit.
///
/// The scan area is: `slot_table_guard` (guard=0) + `slot_table[0..255]` + `slot_table_sentinel` (sentinel=-1).
/// The guard at index 0 is always 0, so allocations start from index 1.
/// The sentinel (-1) at index 256 terminates the scan with failure.
unsafe fn palette_slot_alloc(base: &mut DisplayBase<*const DisplayGfxVtable>, count: i32) -> i32 {
    let count = count as usize;
    let table = &base.slot_table_guard as *const u32;
    // Total entries: slot_table_guard(1) + slot_table(255) + slot_table_sentinel(1) = 257
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

/// Initialize a PaletteContext with a palette index range — port of FUN_00541170 + FUN_005411a0.
///
/// Sets dirty_range_min/max, fills the free stack with descending indices,
/// clears in_use flags and cache, then clears the dirty flag.
unsafe fn palette_context_init(ctx: *mut PaletteContext, range_min: i16, range_max: i16) {
    (*ctx).dirty_range_min = range_min;
    (*ctx).dirty_range_max = range_max;

    // PaletteContext__Init (0x5411A0)
    let range_size = range_max - range_min + 1;
    (*ctx).cache_count = 0;
    (*ctx).free_count = range_size;

    // Fill free_stack with [range_max, range_max-1, ..., range_min]
    if range_size > 0 {
        for i in 0..range_size as usize {
            (*ctx).free_stack[i] = (range_max as u8).wrapping_sub(i as u8);
        }
    }

    (*ctx).cache_iter = 0;
    core::ptr::write_bytes((*ctx).in_use.as_mut_ptr(), 0, 256);

    // FUN_00541170 epilogue
    (*ctx).dirty = 0;
}

/// Port of DisplayGfx::SetActiveLayer (vtable slot 5, 0x523270).
///
/// Returns the layer context pointer for `layer` (valid: 1, 2, 3), or null
/// if the layer index is out of range. The returned pointer is used as
/// palette data input for `update_palette`.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
pub unsafe extern "thiscall" fn set_active_layer(this: *mut DisplayGfx, layer: i32) -> *mut u8 {
    match Layer::try_from_i32(layer) {
        Some(layer) => (*this).base.layer_contexts[layer.idx()] as *mut u8,
        None => core::ptr::null_mut(),
    }
}

/// Port of DisplayGfx::UpdatePalette (vtable slot 24, 0x56A610).
///
/// Updates DisplayGfx palette entries from a `PaletteContext`. The context's
/// `cache` array lists which palette indices to copy, and `cache_count` says
/// how many. Each index's RGB is read from `rgb_table` and written to the
/// DisplayGfx `palette_entries` table.
///
/// If `commit != 0`, calls the palette commit function (0x56CD20) to push
/// the updated entries to the DDraw surface palette.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
/// `palette_ctx` must point to a valid `PaletteContext`.
pub unsafe extern "thiscall" fn update_palette(
    this: *mut DisplayGfx,
    palette_ctx: *mut PaletteContext,
    commit: i32,
) {
    let ctx = &mut *palette_ctx;

    // Reset iteration counter
    ctx.cache_iter = 0;

    if ctx.cache_count <= 0 {
        return;
    }

    let dirty_min = ctx.dirty_range_min as i32;
    let dirty_max = ctx.dirty_range_max as i32;

    // Mark iteration started
    ctx.cache_iter = 1;

    // First index to update
    let mut idx = ctx.cache[0] as usize;

    loop {
        // Read RGB entry from PaletteContext rgb_table (stored as u32: low 3 bytes = R, G, B)
        let rgb = ctx.rgb_table[idx].to_le_bytes();

        // Write to DisplayGfx palette_entries: [R, G, B, flags=0]
        (*this).palette_entries[idx * 4] = rgb[0];
        (*this).palette_entries[idx * 4 + 1] = rgb[1];
        (*this).palette_entries[idx * 4 + 2] = rgb[2];
        (*this).palette_entries[idx * 4 + 3] = 0;

        // Check if we've processed all entries
        if ctx.cache_iter >= ctx.cache_count {
            break;
        }

        // Advance to next index
        idx = ctx.cache[ctx.cache_iter as usize] as usize;
        ctx.cache_iter += 1;
    }

    // Track dirty palette range (expand to cover this update)
    if ((*this).palette_dirty_min as i32) > dirty_min {
        (*this).palette_dirty_min = dirty_min as u32;
    }
    if ((*this).palette_dirty_max as i32) < dirty_max {
        (*this).palette_dirty_max = dirty_max as u32;
    }

    // Commit palette to DDraw surface if requested
    if commit != 0 {
        palette_commit(this);
        (*this).palette_dirty_min = 0x100;
        (*this).palette_dirty_max = 0xFFFF_FFFF;
    }
}

/// Call WA's palette commit function (0x56CD20).
///
/// Usercall: EAX = dirty_min, EDX = dirty_max, stack param = this (DisplayGfx*).
/// Pushes updated palette entries to the DDraw surface palette.
unsafe fn palette_commit(gfx: *mut DisplayGfx) {
    let dirty_min = (*gfx).palette_dirty_min;
    let dirty_max = (*gfx).palette_dirty_max;
    palette_commit_bridge(
        gfx as *mut u8,
        dirty_min,
        dirty_max,
        crate::rebase::rb(0x0056_CD20),
    );
}

#[unsafe(naked)]
unsafe extern "cdecl" fn palette_commit_bridge(
    _gfx: *mut u8,
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

/// Port of DisplayGfx::SetLayerVisibility (vtable slot 23, 0x56A5D0).
///
/// Gets the layer context via `set_active_layer`, then updates the palette
/// from that context if it exists. If `visible < 0`, clears the layer's
/// visibility flag.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
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

/// Construct a Sprite in-place — pure Rust port of ConstructSprite (0x4FAA30).
///
/// Sets vtable, embedded BitGrid sub-object, and context pointer.
/// All other fields are expected to be zeroed by the caller's allocation.
///
/// # Safety
/// `sprite` must point to a zeroed `Sprite`-sized allocation.
pub unsafe fn construct_sprite(sprite: *mut Sprite, sprite_cache: *mut SpriteCache) {
    use crate::bitgrid::{BitGridDisplayVtable, BIT_GRID_DISPLAY_VTABLE};
    use crate::rebase::rb;

    (*sprite).vtable = rb(va::SPRITE_VTABLE) as *const SpriteVtable;
    (*sprite).context_ptr = sprite_cache;

    // Initialize embedded DisplayBitGrid sub-object
    (*sprite).bitgrid.vtable = rb(BIT_GRID_DISPLAY_VTABLE) as *const BitGridDisplayVtable;
    (*sprite).bitgrid.external_buffer = 1; // sprite doesn't own pixel data
    (*sprite).bitgrid.cells_per_unit = 8; // 8bpp pixel buffer
}

/// Port of DisplayGfx::LoadSprite (vtable slot 31, 0x523400).
///
/// Loads a Sprite from VFS into the sprite table. Allocates a Sprite object,
/// constructs it, and loads data via LoadSpriteFromVfs. On success, stores
/// in sprite_ptrs[id] and updates layer metadata.
///
/// The `flag` parameter controls max_frames clamping on the loaded sprite.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
/// `load_sprite_from_vfs` must be a valid function pointer to 0x4FAAF0.
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
    // Bit 23 set = already loaded sentinel
    if id & 0x80_0000 != 0 {
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

    // Already loaded?
    if is_sprite_loaded(this, id as i32) != 0 {
        return 0;
    }

    let sprite = wa_malloc_struct_zeroed::<Sprite>();
    if sprite.is_null() {
        return 0;
    }
    construct_sprite(sprite, base.sprite_cache);

    // Load from VFS
    let result = load_sprite_from_vfs(sprite, gfx_dir, _name, layer_ctx);
    if result == 0 {
        // Load failed — destroy sprite via vtable[0]
        if !sprite.is_null() {
            let dtor = (*(*sprite).vtable).destructor;
            dtor(sprite, 1);
        }
        return 0;
    }

    // Store in sprite table
    let base = &mut (*this).base;
    base.sprite_ptrs[id as usize] = sprite;
    base.sprite_layers[id as usize] = layer.as_u32();
    base.layer_visibility[layer.idx()] += 1;

    // Update max_frames on the sprite if flag is set
    if flag != 0 {
        let sprite = base.sprite_ptrs[id as usize];
        let id_u16 = id as u16;
        if id_u16 != 0 && id_u16 < (*sprite).max_frames {
            (*sprite).max_frames = id_u16;
        }
        (*sprite)._unknown_18 = (id >> 16) as u16;
    }

    1
}

/// Port of FUN_005733b0 — load sprite data from GfxDir stream.
///
/// Original convention: `usercall(EDI=sprite, ECX=gfx_dir) + stack(palette_ctx, name), RET 0x8`.
/// Ported to a regular Rust function — no usercall bridge needed.
///
/// Reads sprite header, palette, and frame pixel data from a `.dir` archive stream.
/// In headless mode (g_DisplayModeFlag != 0), skips all surface creation.
///
/// # Safety
/// All pointers must be valid. `sprite` must be a zeroed 0x70-byte allocation.
pub unsafe fn load_sprite_by_name(
    sprite: *mut LayerSprite,
    gfx_dir: *mut GfxDir,
    palette_ctx: *mut PaletteContext,
    name: *const c_char,
) -> i32 {
    use crate::address::va;
    use crate::rebase::rb;
    use crate::render::display::context::{FastcallResult, RenderContext};
    use crate::render::palette::{palette_map_color, remap_pixels_through_lut};
    use crate::render::sprite::gfx_dir::{call_gfx_load_image, GfxDirStream};

    use crate::wa_alloc::wa_malloc;

    let sp = sprite as *mut u8;

    // 1. Copy name into sprite.name (max 0x4F chars + null)
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

    // 2. Store gfx_dir and palette_ctx in sprite
    (*sprite).gfx_dir = gfx_dir;
    (*sprite).palette_ctx = palette_ctx;

    // 3. Load image stream from GfxDir
    let stream = call_gfx_load_image(gfx_dir, name) as *mut GfxDirStream;
    if stream.is_null() {
        return 0;
    }

    // 5. Check headless mode — skip all surface creation if g_DisplayModeFlag != 0
    let display_mode_flag = *(rb(va::G_DISPLAY_MODE_FLAG) as *const u8);
    if display_mode_flag == 0 {
        // ── Graphics path: read header, palette, allocate surfaces ──

        // Read and discard: remaining() result (original calls vtable[2])
        GfxDirStream::remaining_raw(stream);

        // Read .spr header as 4+4+2+2 separate calls, matching the original exactly.
        // The stream seeks before each read; matching the original's read granularity
        // may matter for internal stream state.
        let mut hdr4 = [0u8; 4];
        GfxDirStream::read_raw(stream, hdr4.as_mut_ptr(), 4); // unused/version
        GfxDirStream::read_raw(stream, hdr4.as_mut_ptr(), 4); // data_size

        let mut header_flags: u16 = 0;
        GfxDirStream::read_raw(stream, &mut header_flags as *mut u16 as *mut u8, 2);

        let mut palette_count: u32 = 0;
        GfxDirStream::read_raw(stream, &mut palette_count as *mut u32 as *mut u8, 2);

        // Build palette LUT: bulk-read all RGB triplets then iterate.
        let mut palette_lut = [0u8; 256];
        let lut_count = (palette_count as usize).min(256);
        let bulk_size = palette_count as usize * 3;
        let mut palette_data = [0u8; 768]; // max 256 * 3
        GfxDirStream::read_raw(stream, palette_data.as_mut_ptr(), bulk_size as u32);

        // Palette entry 0 is always transparent (display index 0).
        // The palette RGB data in the file defines entries 1..palette_count,
        // NOT entry 0. So palette_data[0..3] maps to lut[1], etc.
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

        // Read sprite metadata fields (in original WA's read order:
        // field_60 first, then flags / cell_width / cell_height).
        GfxDirStream::read_raw(stream, sp.add(0x60), 4); // field_60
        GfxDirStream::read_raw(stream, sp.add(0x64), 2); // flags
        GfxDirStream::read_raw(stream, sp.add(0x68), 2); // cell_width
        GfxDirStream::read_raw(stream, sp.add(0x6A), 2); // cell_height

        // frame_count: zero first, then read
        (*sprite).frame_count = 0;
        GfxDirStream::read_raw(stream, sp.add(0x66), 2);
        let frame_count = (*sprite).frame_count as usize;

        // Allocate LayerSpriteFrame array (counted array: count at [-4])
        // Size per element: 0x14, with 4-byte count prefix
        let total_elems = frame_count;
        // Overflow check matching original (saturate on overflow)
        let checked_count = total_elems as u32;
        let checked_size = checked_count.checked_mul(0x14).unwrap_or(u32::MAX);
        let checked_alloc = checked_size.checked_add(4).unwrap_or(u32::MAX);

        let array_base = wa_malloc(checked_alloc);
        let frame_array = if !array_base.is_null() {
            *(array_base as *mut u32) = checked_count; // store count at [-4]
            let arr = array_base.add(4);
            // Construct each element: set vtable at +0x08, zero surface at +0x0C
            let bitmap_vtable = rb(0x00643F64) as u32; // CBitmap vtable (set by constructor at 0x573C30)
            for j in 0..total_elems {
                let elem = arr.add(j * 0x14);
                *(elem.add(0x08) as *mut u32) = bitmap_vtable;
                *(elem.add(0x0C) as *mut u32) = 0;
            }
            arr
        } else {
            core::ptr::null_mut()
        };
        (*sprite).frame_array = frame_array as *mut LayerSpriteFrame;

        // Skip alignment padding: while (remaining() & 3) != 0, read 1 dummy byte
        loop {
            let remaining = GfxDirStream::remaining_raw(stream);
            if remaining & 3 == 0 {
                break;
            }
            let mut dummy = 0u8;
            GfxDirStream::read_raw(stream, &mut dummy, 1);
        }

        // Read frame headers
        if frame_count > 0 && !frame_array.is_null() {
            for j in 0..frame_count {
                let elem = frame_array.add(j * 0x14);
                // Read 4-byte unknown header (discarded)
                let mut frame_hdr = [0u8; 4];
                GfxDirStream::read_raw(stream, frame_hdr.as_mut_ptr(), 4);
                // Read 4x u16: start_x, start_y, end_x, end_y
                GfxDirStream::read_raw(stream, elem, 2); // start_x
                GfxDirStream::read_raw(stream, elem.add(2), 2); // start_y
                GfxDirStream::read_raw(stream, elem.add(4), 2); // end_x
                GfxDirStream::read_raw(stream, elem.add(6), 2); // end_y
            }
        }

        // Surface creation loop: create surfaces and read pixel data for each frame
        if frame_count > 0 && !frame_array.is_null() {
            let render_ctx = *(rb(va::G_RENDER_CONTEXT) as *const *mut RenderContext);

            for j in 0..frame_count {
                let elem = frame_array.add(j * 0x14);
                let start_x = *(elem as *const i16) as i32;
                let start_y = *(elem.add(2) as *const i16) as i32;
                let end_x = *(elem.add(4) as *const i16) as i32;
                let end_y = *(elem.add(6) as *const i16) as i32;

                let width = end_x - start_x;
                let height = end_y - start_y;

                if width * height == 0 {
                    continue;
                }

                // Ensure surface exists at elem+0x0C
                // alloc_surface returns the surface pointer in EAX (the return value),
                // NOT via the FastcallResult buffer. The original code at 0x57367f
                // does: MOV [EBX+0xC], EAX — storing EAX directly.
                let surface_ptr = elem.add(0x0C) as *mut u32;
                if *surface_ptr == 0 {
                    let mut buf = FastcallResult::default();
                    let ret = RenderContext::alloc_surface_raw(render_ctx, &mut buf);
                    *surface_ptr = ret as u32;
                }
                let surface = *surface_ptr as *mut u8;
                if surface.is_null() {
                    continue;
                }

                // Init surface: surface->vtable[5](width, height, 0)
                {
                    let vt = *(surface as *const *const u32);
                    let init_fn: unsafe extern "fastcall" fn(
                        *mut u8,
                        *mut FastcallResult,
                        u32,
                        u32,
                        u32,
                    ) = core::mem::transmute(*vt.add(5));
                    let mut buf = FastcallResult::default();
                    init_fn(surface, &mut buf, width as u32, height as u32, 0);
                }

                // SetColorKey: surface->vtable[7](0, 0x10)
                {
                    let vt = *(surface as *const *const u32);
                    let set_ck_fn: unsafe extern "fastcall" fn(
                        *mut u8,
                        *mut FastcallResult,
                        u32,
                        u32,
                    ) = core::mem::transmute(*vt.add(7));
                    let mut buf = FastcallResult::default();
                    set_ck_fn(surface, &mut buf, 0, 0x10);
                }

                // Lock: surface->vtable[3](&out_data, &out_pitch)
                let mut data_ptr: u32 = 0;
                let mut pitch: u32 = 0;
                {
                    let vt = *(surface as *const *const u32);
                    let lock_fn: unsafe extern "fastcall" fn(
                        *mut u8,
                        *mut FastcallResult,
                        *mut u32,
                        *mut u32,
                    ) = core::mem::transmute(*vt.add(3));
                    let mut buf = FastcallResult::default();
                    lock_fn(surface, &mut buf, &mut data_ptr, &mut pitch);
                }

                if data_ptr != 0 && pitch != 0 {
                    // Read pixel data row by row
                    let data = data_ptr as *mut u8;
                    for row in 0..height {
                        let row_dest = data.add((row as u32 * pitch) as usize);
                        GfxDirStream::read_raw(stream, row_dest, width as u32);
                    }

                    // Remap pixels through palette LUT
                    let width_dwords = ((width as u32) + 3) / 4;
                    remap_pixels_through_lut(
                        data,
                        pitch,
                        palette_lut.as_ptr(),
                        width_dwords,
                        height as u32,
                    );
                }

                // Unlock: surface->vtable[4](data_ptr)
                {
                    let vt = *(surface as *const *const u32);
                    let unlock_fn: unsafe extern "fastcall" fn(*mut u8, *mut FastcallResult, u32) =
                        core::mem::transmute(*vt.add(4));
                    let mut buf = FastcallResult::default();
                    unlock_fn(surface, &mut buf, data_ptr);
                }
            }
        }
    }

    // Destroy stream reader
    GfxDirStream::destroy_raw(stream);
    1
}

/// Free a LayerSprite and its associated surfaces.
///
/// Port of FUN_0056a2f0 (usercall EDI=sprite, plain RET).
/// Destroys each LayerSpriteFrame's surface via `surface->vtable[0](1)`,
/// frees the counted array, then frees the sprite itself.
///
/// # Safety
/// `sprite` must be a valid LayerSprite pointer allocated via `wa_malloc`.
pub unsafe fn free_layer_sprite(sprite: *mut LayerSprite) {
    use crate::wa_alloc::wa_free;

    let frame_array = (*sprite).frame_array as *mut u8;
    if !frame_array.is_null() {
        let count_ptr = (frame_array as *mut u32).sub(1);
        let count = *count_ptr as usize;

        // Destroy each frame's surface in reverse order
        // (matches eh_vector_destructor_iterator behavior)
        for i in (0..count).rev() {
            let elem = frame_array.add(i * 0x14);
            let surface = *(elem.add(0x0C) as *const u32);
            if surface != 0 {
                let vt = *(surface as *const *const u32);
                let dtor: unsafe extern "thiscall" fn(u32, u32) = core::mem::transmute(*vt);
                dtor(surface, 1);
            }
        }

        wa_free(count_ptr);
    }

    wa_free(sprite);
}

/// Port of DisplayGfx::LoadSpriteByLayer (vtable slot 37, 0x56A4C0).
///
/// Simplified sprite loading that stores into DisplayGfx::sprite_table
/// (offset 0x3DD4) instead of DisplayBase::sprite_ptrs. Allocates a raw
/// 0x70-byte LayerSprite, partially initializes it, then loads via
/// `load_sprite_by_name` (pure Rust port of FUN_005733b0).
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`.
pub unsafe fn load_sprite_by_layer(
    this: *mut DisplayGfx,
    layer: u32,
    id: u32,
    gfx_dir: *mut GfxDir,
    name: *const c_char,
) -> i32 {
    use crate::wa_alloc::wa_malloc_zeroed;

    // Bit 23 set = already loaded sentinel
    if id & 0x80_0000 != 0 {
        return 1;
    }

    // Call set_active_layer (vtable slot 5) — returns the layer's
    // PaletteContext, or null if `layer` is out of range.
    let palette_ctx = set_active_layer(this, layer as i32) as *mut PaletteContext;

    if !is_valid_sprite_id(id as i32) {
        return 0;
    }

    // Already loaded?
    if is_sprite_loaded(this, id as i32) != 0 {
        return 1;
    }

    // Allocate 0x70 bytes + 0x20 guard (matching WA_MallocMemset behavior)
    let sprite = wa_malloc_zeroed(0x90) as *mut LayerSprite;
    if sprite.is_null() {
        return 0;
    }

    // Partial init (NOT ConstructSprite — different from load_sprite)
    (*sprite).display_gfx = this;
    (*sprite).frame_count = 0;
    (*sprite).frame_array = core::ptr::null_mut();
    (*sprite).gfx_dir = core::ptr::null_mut();

    // Load sprite data — pure Rust port of FUN_005733b0
    let result = load_sprite_by_name(sprite, gfx_dir, palette_ctx, name);
    if result == 0 {
        free_layer_sprite(sprite);
        return 0;
    }

    // Store in sprite_table (DisplayGfx offset 0x3DD4)
    (*this).sprite_table[id as usize] = sprite;

    1
}

// GetSpriteFrameForBlit (vtable slot 33, 0x5237C0) is NOT ported — it is
// hot-path though, called from our own `blit_sprite` (slot 19) on every
// sprite render via the raw vtable pointer. The two inner helpers
// (Sprite__GetFrameForBlit at 0x4FAD30, SpriteBank__GetFrameForBlit at
// 0x4F9710) use ESI for the sprite/bank receiver in a usercall convention
// that's impractical to bridge directly. Porting requires first porting
// the FrameCache + decompression chain (FUN_004FA950 → FUN_005B29E0).

/// Port of `DisplayGfx::LoadFont` (vtable slot 34, 0x523560).
///
/// Validates `layer` (1..=3) and `font_id` (1..=31), then allocates a
/// zero-initialized 0x1C-byte `FontObject`, loads the named resource via
/// `font_load_from_gfx`, and stores the object in `DisplayBase::font_table`.
///
/// **`layer` is the WA "mode" parameter, but it's the same value space as
/// the layer index everywhere else** — it indexes `layer_contexts[1..=3]`
/// and `layer_visibility[1..=3]` exactly the way `set_layer_color` and
/// `set_active_layer` do. The shipping game only ever passes `1`.
///
/// On load failure, the partially-initialized font object is leaked to match
/// the original's behavior (it calls a sprite-bank-style cleanup helper
/// `FUN_005230c0` which is not exercised here).
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`. `_gfx` must be a GfxDir pointer
/// (or null if only the cached path is needed). `filename` must be a valid
/// null-terminated C string.
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

    // Validate font_id (1..=31) and that the slot is empty.
    if !(1..=31).contains(&font_id) {
        return 0;
    }
    if !base.font_table[font_id as usize].is_null() {
        return 0;
    }

    // Allocate zeroed FontObject. The original uses WA_MallocMemset(0x1C)
    // which only memsets the requested size; wa_malloc_struct_zeroed matches.
    let font_obj = wa_malloc_struct_zeroed::<Font>();
    if font_obj.is_null() {
        return 0;
    }

    // Load and parse the font data.
    let result = font_load_from_gfx(font_obj, gfx_dir, layer_ctx, filename);
    if result == 0 {
        // Original leaks here on failure (via an unported cleanup helper).
        // We match that behavior rather than introducing a free path that
        // might differ from WA's.
        return 0;
    }

    // Install into font_table. The original also records which layer owns
    // this font slot at DisplayBase + 0x301C + font_id*4 (the gap region
    // next to font_table), then bumps the layer visibility counter.
    base.font_table[font_id as usize] = font_obj;
    base.font_layers[font_id as usize] = layer.as_u32();
    base.layer_visibility[layer.idx()] += 1;

    1
}

/// Port of `DisplayGfx::LoadFontExtension` (vtable slot 35, 0x523620).
///
/// Validates `font_id`, looks up the existing font object, resolves the
/// palette context for the font's mode (recorded in `_gap_301c` by `load_font`),
/// resolves the RGB color from `palette_value` via `palette_context_lookup_entry`,
/// and dispatches to `font_extend`.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`. `path` and `char_map` must be
/// valid null-terminated C strings.
pub unsafe fn load_font_extension(
    this: *mut DisplayGfx,
    font_id: i32,
    path: *const c_char,
    char_map: *const u8,
    palette_value: u32,
    _flag: i32,
) -> u32 {
    use crate::render::palette::palette_context_lookup_entry;

    if !(1..=31).contains(&font_id) {
        return 0;
    }
    let base = &mut (*this).base;
    let font_obj_addr = base.font_table[font_id as usize];
    if font_obj_addr.is_null() {
        return 0;
    }
    let font_obj = font_obj_addr as *mut Font;

    // Resolve the RGB color via the layer-1 palette context (DisplayBase+0x3120).
    // The original always reads layer_contexts[1] here, regardless of which
    // mode owns the font. This matches the disassembly at 0x52364d:
    //   `MOV ECX, [EDI+0x3120]` (= layer_contexts[1])
    let layer1_ctx = base.layer_contexts[Layer::ONE.idx()];
    let mut resolved_rgb: u32 = 0;
    let _ = palette_context_lookup_entry(layer1_ctx, palette_value as i32, &mut resolved_rgb);

    // The actual font extension call uses the font's owning layer's palette
    // ctx. The original reads `layer_contexts[font_layers[font_id]]` directly
    // without validation; if `font_layers` is 0 it reads index 0 (always
    // null on a properly-initialized DisplayBase). We preserve that exact
    // index lookup rather than going through `Layer` here.
    let layer_idx = base.font_layers[font_id as usize] as usize;
    let layer_ctx = base.layer_contexts[layer_idx];

    font_extend(font_obj, layer_ctx, path, char_map, resolved_rgb);

    1
}

/// Port of `DisplayGfx::GetFontInfo` (vtable slot 8, 0x523790).
///
/// Validates `font_id` (must be in `1..=31` with a non-null entry in
/// `font_table`), then dispatches to `font_get_info_impl`. The original
/// passes `out_2` via `EDX` and `out_1` via `EDI`; this port preserves
/// that mapping (`out_1` = max metric, `out_2` = font max width).
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`. The output pointers must be
/// writable.
pub unsafe extern "thiscall" fn get_font_info(
    this: *mut DisplayGfx,
    font_id: i32,
    out_1: *mut u32,
    out_2: *mut u32,
) -> u32 {
    if !(1..=31).contains(&font_id) {
        return 0;
    }
    let font_obj = (*this).base.font_table[font_id as usize] as *const Font;
    if font_obj.is_null() {
        return 0;
    }
    font_get_info_impl(font_obj, out_1 as *mut i32, out_2 as *mut i32)
}

/// Port of `DisplayGfx::GetFontMetric` (vtable slot 9, 0x523750).
///
/// Validates `font_id`, then dispatches to `font_get_metric_impl`.
/// `char_code` is truncated to 8 bits to match the original's `MOV AL, ...`
/// register usage. The original passes `out_1` via `EDX` and `out_2` via
/// `EDI`, so `out_1` receives the per-character metric and `out_2`
/// receives the font's max width.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`. The output pointers must be
/// writable.
pub unsafe extern "thiscall" fn get_font_metric(
    this: *mut DisplayGfx,
    font_id: i32,
    char_code: u32,
    out_1: *mut u32,
    out_2: *mut u32,
) -> u32 {
    if !(1..=31).contains(&font_id) {
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

/// Port of `DisplayGfx::SetFontParam` (vtable slot 10, 0x523710).
///
/// Validates `font_id`, then dispatches to `font_set_param_impl`. Per the
/// original's register shuffle: `p3` is the input string, `p4` is the
/// output total advance, and `p5` is the output font max width.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`. `p3` must be a valid
/// null-terminated byte string. `p4` and `p5` must be writable `*mut i32`.
pub unsafe extern "thiscall" fn set_font_param(
    this: *mut DisplayGfx,
    font_id: i32,
    p3: u32,
    p4: u32,
    p5: u32,
) -> u32 {
    if !(1..=31).contains(&font_id) {
        return 0;
    }
    let font_obj = (*this).base.font_table[font_id as usize] as *const Font;
    if font_obj.is_null() {
        return 0;
    }
    font_set_param_impl(font_obj, p3 as *const u8, p4 as *mut i32, p5 as *mut i32);
    1
}

/// Port of `DisplayGfx::SetFontPalette` (vtable slot 36, 0x523690).
///
/// Despite the name, this is the entry point for `font_set_palette_impl` —
/// see that function's doc for what it actually does (it extends the
/// digital font with derived `'.'` and `';'` glyphs). The wrapper is just
/// a thin index lookup into `font_table`; the original has no bounds or
/// null check on the index, so we mirror that.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx` and `font_index` must be a
/// valid index into `font_table` whose entry is a non-null `FontObject`.
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
/// The original splits a glyph into chunks of `min(remaining_width, 8)`
/// pixels and dispatches to a helper that copies that many bytes per row,
/// stepping `dst_stride`/`src_stride` per row. **No transparency, no
/// blending** — every byte (palette index) is copied verbatim. Verified
/// against helpers 1 (1-byte/row) and 8 (8-byte/row).
///
/// Collapses the entire dispatch + 9 helpers into a single nested loop.
///
/// # Safety
/// `dst` and `src` must be valid pointers to buffers of at least
/// `(height-1)*stride + width` bytes (with their respective strides).
/// `width` and `height` must be non-negative.
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
/// Walks `msg` and rasterizes each character into the destination
/// `BitGrid`. Glyph rows are copied directly with no transparency: the
/// destination byte is overwritten with the source byte even when the
/// source byte is 0. (The font's "background" pixels carry whatever
/// palette index the font baked into the .fnt file.)
///
/// **Calling convention details that match the original exactly:**
///
/// - The first thing the function does is `*out_width = font.width`
///   unconditionally — including on the early validation-failure paths.
/// - On success, returns the number of characters successfully drawn
///   (forward) or `-1` (right-aligned, full string).
/// - On truncation (next glyph wouldn't fit), returns the index of the
///   first character that wasn't drawn.
/// - On early validation failure (negative pen, doesn't fit vertically),
///   returns 0 — but `*out_width` has already been written.
/// - `out_pen_x` is initialised to 0 and updated as the running advance
///   (how many pixels of horizontal space the text consumed).
///
/// **Glyph source row stride** is `glyph.width` for base-font glyphs but
/// `font.width` for extension glyphs (those with index `>= font._height2`,
/// added by `font_extend` or `font_set_palette_impl`). Extension glyphs
/// live in a separate buffer with uniform `font.width`-byte rows; base
/// glyphs are tightly packed in the main `.fnt` pixel data.
///
/// `font_id_high` is the *high 16 bits* of the slot 7 caller's `font_id`,
/// sign-extended (the slot wrapper does `SAR EAX, 0x10`). Bit 1 selects
/// right-aligned mode; other bits are ignored by the rasterizer.
///
/// # Safety
/// `font_obj` must be a valid `*const FontObject`. `bitmap` must be a
/// valid `*const BitGrid` whose `data`/`row_stride`/`width`/`height`
/// describe a backing buffer large enough for the requested glyph rect.
/// `msg` must be a valid null-terminated byte string. `out_pen_x` and
/// `out_width` must be writable.
pub unsafe fn font_draw_text_impl(
    font_obj: *const Font,
    bitmap: *const BitGrid,
    pen_x: i32,
    pen_y: i32,
    msg: *const u8,
    out_pen_x: *mut i32,
    out_width: *mut i32,
    font_id_high: i32,
) -> i32 {
    let font = &*font_obj;
    let font_width = font.width as i16 as i32;

    // Always write font width to out_width — even on validation failure.
    *out_width = font_width;

    let bm = &*bitmap;
    let bitmap_width = bm.width as i32;
    let bitmap_height = bm.height as i32;
    let stride = bm.row_stride as i32;

    // Validation: pen must be non-negative AND a glyph must fit vertically.
    if pen_x < 0 || pen_y < 0 || font_width + pen_y > bitmap_height {
        return 0;
    }

    // Pre-adjust the data pointer to the (pen_x, pen_y) origin so each
    // glyph dst calculation only needs the per-glyph offset.
    let data_origin = bm.data.offset((pen_y * stride + pen_x) as isize);

    let height2 = font._height2 as i16 as i32;
    let char_to_glyph = font.char_to_glyph_idx;
    let glyph_table = font.glyph_table;
    let pixel_data = font.pixel_data;
    let width_div_5 = font.width_div_5 as i16 as i32;

    *out_pen_x = 0;

    if (font_id_high >> 1) & 1 != 0 {
        // -----------------------------------------------------------------
        // Right-aligned path: walk msg right-to-left, advance leftward.
        // -----------------------------------------------------------------
        let mut len: i32 = 0;
        while *msg.offset(len as isize) != 0 {
            len += 1;
        }
        let mut idx = len - 1;
        if idx < 0 {
            return idx; // -1 for empty string
        }

        loop {
            let ch = *msg.offset(idx as isize) as usize;
            let glyph_idx_1based = *char_to_glyph.add(ch);
            if glyph_idx_1based == 0 {
                // Unmapped char: just advance.
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
                return idx; // -1 = full string drawn
            }
        }
    } else {
        // -----------------------------------------------------------------
        // Forward path: walk msg left-to-right, advance rightward.
        // -----------------------------------------------------------------
        if *msg == 0 {
            return 0;
        }
        let mut idx: i32 = 0;
        loop {
            let cur_advance = *out_pen_x;
            // Outer fit check: stop if next char would start past the bitmap.
            if cur_advance + pen_x >= bitmap_width {
                return idx;
            }

            let ch = *msg.offset(idx as isize) as usize;
            let glyph_idx_1based = *char_to_glyph.add(ch);
            if glyph_idx_1based == 0 {
                // Unmapped char: just advance.
                *out_pen_x = cur_advance + width_div_5;
            } else {
                let glyph_idx = glyph_idx_1based as i32 - 1;
                let glyph = &*glyph_table.add(glyph_idx as usize);
                let glyph_width = glyph.width as i32;
                let glyph_height = glyph.height as i32;

                // Inner fit check: glyph must end before bitmap.width-2.
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

/// Port of `DisplayGfx::DrawTextOnBitmap` (vtable slot 7, 0x5236B0).
///
/// Splits `font_id` into `font_id_low` (1-based slot, validated against
/// `1..=31`) and `font_id_high` (sign-extended flags). Looks up the
/// `FontObject` from `font_table[font_id_low]`, dispatches to
/// `font_draw_text_impl`. The original is sloppy about validation: if
/// `font_id_low` is out of range OR the slot is null, it returns 0 WITHOUT
/// writing `*out_width`. We mirror that exactly.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`. `bitmap`, `msg`, `out_pen_x`,
/// `out_width` must satisfy `font_draw_text_impl`'s contract on the
/// success path.
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
    let font_id_low = (font_id as u32) & 0xFFFF;
    if !(1..=31).contains(&font_id_low) {
        return 0;
    }
    let font_obj = (*this).base.font_table[font_id_low as usize] as *const Font;
    if font_obj.is_null() {
        return 0;
    }
    // Sign-extend the high half (matches `SAR EAX, 0x10` in the wrapper).
    let font_id_high = (font_id as i32) >> 16;
    font_draw_text_impl(
        font_obj,
        bitmap as *const BitGrid,
        pen_x,
        pen_y,
        msg as *const u8,
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
/// the third stack arg. The slot reads four fields out of this struct;
/// the rest of the layout is unknown.
///
/// Field offsets verified by tracing the disassembly of slot 11
/// (`0x56b8c0`): the function reads `[ECX+0x14]` (bpp dispatch),
/// `[ECX+0x18]` (total height), `[ECX+0x10]` (row stride), and
/// `[ECX+0x08]` (source data base pointer).
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

/// Pure-Rust port of `FUN_005B2A5E` (the 8bpp 64-byte row replicator).
///
/// For each of `row_count` rows, reads 8 bytes from `src` and writes them
/// 8 times consecutively to `dst` (filling 64 bytes per row), then advances
/// `dst` by `dst_stride` and `src` by exactly 8 bytes (regardless of any
/// caller-side row stride). The original is hand-unrolled in asm with 16
/// dword writes per row; the port collapses to one inner loop.
///
/// Used by `DisplayGfx::DrawTiledBitmap` (slot 11) for the 8bpp tile
/// populate phase. Has only that single caller (verified via
/// `get_xrefs_to FUN_005B2A5E`).
///
/// # Safety
/// `dst` must point to a buffer with at least `(row_count - 1) * dst_stride
/// + 64` bytes; `src` must point to a buffer with at least `row_count * 8`
/// bytes. `row_count` must be positive (the original loops at least once
/// before checking; we mirror that with `>= 1`).
unsafe fn blit_64byte_row_pattern(
    mut dst: *mut u8,
    dst_stride: i32,
    mut src: *const u8,
    row_count: i32,
) {
    let mut remaining = row_count;
    while remaining > 0 {
        // Read 8 source bytes (one row's pattern).
        let pattern_lo = (src as *const u32).read_unaligned();
        let pattern_hi = (src.add(4) as *const u32).read_unaligned();

        // Write 16 dwords = 64 bytes by replicating (lo, hi) 8 times.
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

/// Pure-Rust port of `BlitColorTable_Forward` (`0x5B2B5D`).
///
/// Transparent byte-level blit: for each pixel in a `width × height` rect,
/// if the source byte is non-zero, write it to the destination; otherwise
/// leave the destination unchanged. Walks rows with independent
/// `dst_stride`/`src_stride`.
///
/// Used by `DisplayGfx::DrawTiledBitmap` (slot 11) for the `0x40`-bpp tile
/// populate phase. The other listed caller in WA — `BitGrid::BlitSpriteRect`
/// at `0x4F6C93` — is already replaced by our pure-Rust `blit_sprite_rect`
/// in `sprite_blit.rs`, so this primitive is exclusively reachable through
/// slot 11 in our build.
///
/// # Safety
/// `dst`/`src` must point to buffers large enough for the requested rect
/// at the given strides. `width` and `height` must be positive (the
/// original loops once before checking, mirrored here).
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

/// Pure-Rust port of `FUN_00403c60` — the `CBitmap` blit-via-wrapper helper.
///
/// Lazily allocates the backing surface for `cbm` via the render context's
/// `alloc_surface` (slot 22) the first time it is called for this
/// `CBitmap`, then dispatches the blit through `draw_landscape` (slot 23).
///
/// `alloc_surface` returns the surface pointer in EAX (the binding's return
/// value), NOT via the `FastcallResult` buffer — see
/// `feedback_alloc_surface_return.md`.
///
/// # Safety
/// `cbm` must be a valid `*mut CBitmap`. `g_RenderContext` must be
/// initialized.
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

    // Lazy alloc: if the cbm has no surface yet, ask the wrapper for one.
    if (*cbm).surface.is_null() {
        let mut buf = FastcallResult::default();
        let ret = RenderContext::alloc_surface_raw(wrapper, &mut buf);
        // alloc_surface returns the surface pointer in EAX (the bound
        // wrapper's return value).
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

/// Pure-Rust port of `DisplayGfx::BlitBitmapClipped` (`0x56A700`).
///
/// Computes the clipped intersection of `(dst_x, dst_y, width, height)`
/// against the `DisplayBase` clip rect (`0x3550..0x355C`), flushes the
/// render lock if held, and dispatches the actual blit through
/// `cbitmap_blit_via_wrapper`.
///
/// Used by ported slots 11 (`DrawTiledBitmap`), 22 (`DrawTiledTerrain`),
/// and the bitmap-sprite branch of slot 19 (`BlitSprite`) via the DLL
/// `blit_sprite` hook.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx`. `surface` must be a valid
/// `*mut CBitmap`. `g_RenderContext` must be initialized.
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

    // Trivial reject — entirely outside the clip rect.
    if dst_x >= cx2 || dst_x2 <= cx1 || dst_y >= cy2 || dst_y2 <= cy1 {
        return;
    }

    let new_left = cx1.max(dst_x);
    let new_right = cx2.min(dst_x2);
    let new_top = cy1.max(dst_y);
    let new_bottom = cy2.min(dst_y2);

    // Degenerate (zero-width or zero-height clipped rect) — original
    // checks `local_28 != iVar2 && local_24 != iVar1`. Mirror exactly.
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

/// Pure-Rust port of `DisplayGfx::BlitBitmapTiled` (`0x56A7D0`,
/// usercall EAX=initial_x, EDI=tile_width).
///
/// Tiles `surface` horizontally across the visible clip range
/// `[clip_x1, clip_x2)`. The function first walks `initial_x` by
/// `±tile_width` until `x ≤ clip_x1`, then blits at `(x, dst_y),
/// (x + tile_width, dst_y), …` until past `clip_x2`. Each blit goes
/// through `blit_bitmap_clipped_native` with the same fixed `flags = 2`
/// the original WA function uses.
///
/// Used by the bitmap-sprite branch of slot 19 (`BlitSprite`) when the
/// `tiled` mode bit (high_flags bit 16) is set on the sprite.
///
/// # Safety
/// `this` must be a valid `*mut DisplayGfx` with an initialized clip
/// rect. `surface` must be a valid `*mut CBitmap`. `tile_width` must be
/// positive (zero or negative would loop forever; matches the original).
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

    // Walk to the largest x ≤ clip_x1 in the arithmetic sequence
    // {initial_x ± k*tile_width} (matches the original's two-loop pattern
    // at 0x56A7DD..0x56A7F4).
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

/// Pure-Rust port of `DisplayGfx::GetBitmapSpriteInfo` (`0x573C50`,
/// usercall EAX=bitmap_obj, EDX=palette_or_anim).
///
/// Resolves an animation/palette value into a frame index for a
/// `LayerSprite`, then writes the selected frame's bounding box and the
/// sprite's full cell width/height to the output pointers and returns a
/// pointer to the frame's `CBitmap` (the trailing 12 bytes of the
/// `LayerSpriteFrame` entry).
///
/// `bitmap_obj.flags` (`LayerSprite +0x64`) controls interpretation of
/// `palette_or_anim`:
/// - bit 0 set: low 16 bits taken as-is (no clamp)
/// - bit 0 clear: signed clamp to `[0, 0xFFFF]`
/// - bit 1 set: ping-pong (bounce) iteration over `[0, frame_count)`
/// - bit 1 clear: forward iteration
///
/// Used by the bitmap-sprite branch of slot 19 (`BlitSprite`).
///
/// # Safety
/// `bitmap_obj` must be a valid `LayerSprite` with a populated
/// `frame_array`. The output pointers must be writable.
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

    // Step 1: clamp / mask the palette_or_anim value into [0, 0xFFFF].
    let pal: i32 = if flags & 1 != 0 {
        (palette_or_anim & 0xFFFF) as i32
    } else {
        let p = palette_or_anim as i32;
        p.max(0).min(0xFFFF)
    };

    // Step 2: compute frame index from frame_count and pal.
    let frame_count = (*bitmap_obj).frame_count as i16 as i32;
    let frame_idx = if flags & 2 != 0 {
        // Ping-pong: scaled = ((2*frame_count - 1) * pal) >> 16
        // If scaled >= frame_count, fold back: (2*frame_count - scaled) - 1.
        let scaled = ((frame_count * 2 - 1) * pal) >> 16;
        if scaled >= frame_count {
            (frame_count * 2 - scaled) - 1
        } else {
            scaled
        }
    } else {
        // Forward: idx = (frame_count * pal) >> 16
        (frame_count * pal) >> 16
    };

    // Step 3: read the selected frame entry.
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
            // Pre-reserve the entire bitmap_vec in one allocation. The
            // original grows the vector incrementally; we don't.
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
                // Allocate one CBitmap entry.
                let cbm = wa_malloc(core::mem::size_of::<CBitmap>() as u32) as *mut CBitmap;
                if !cbm.is_null() {
                    (*cbm).vtable = cbitmap_vt;
                    (*cbm).surface = core::ptr::null_mut();
                    (*cbm)._pad = 0;
                }
                // The original assumes the malloc succeeded for the rest
                // of the loop body (no null check on `puVar6`). Mirror
                // that exactly.

                let strip_h = remaining.min(0x400);

                // Lazy-alloc the surface (matches the original's
                // double-checked alloc_surface inside the strip loop).
                if (*cbm).surface.is_null() {
                    let mut buf = FastcallResult::default();
                    let s = RenderContext::alloc_surface_raw(wrapper, &mut buf);
                    (*cbm).surface = s as *mut Surface;
                }

                // Init at 0x40 × strip_h × 8bpp.
                let mut init_buf = FastcallResult::default();
                Surface::init_surface_raw((*cbm).surface, &mut init_buf, 0x40, strip_h, 8);

                // On failure, retry with bpp=4. The original re-allocates
                // the surface first (still null-checks), then retries.
                if init_buf.value != 0 {
                    if (*cbm).surface.is_null() {
                        let mut buf = FastcallResult::default();
                        let s = RenderContext::alloc_surface_raw(wrapper, &mut buf);
                        (*cbm).surface = s as *mut Surface;
                    }
                    let mut init_buf2 = FastcallResult::default();
                    Surface::init_surface_raw((*cbm).surface, &mut init_buf2, 0x40, strip_h, 4);
                    if init_buf2.value != 0 {
                        // Both inits failed — bail (matches the original's
                        // `JNZ 0056bcb4` to the function epilogue).
                        return;
                    }
                }

                // push_back: write the entry and bump end (capacity is
                // pre-reserved; this never reallocates).
                *(*this).bitmap_end = cbm;
                (*this).bitmap_end = (*this).bitmap_end.add(1);

                accum += 0x400;
                remaining -= 0x400;
            }
        }
        (*this).tile_cache_populated = 0;
    }

    // -------------------------------------------------------------------
    // Phase 2 — Populate (only if tile_cache_populated == 0)
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

            // Lazy-alloc surface (paranoid: should already be non-null
            // from phase 1, but the original repeats the check).
            if (*cbm).surface.is_null() {
                let wrapper = *(rb(va::G_RENDER_CONTEXT) as *const *mut RenderContext);
                let mut buf = FastcallResult::default();
                let s = RenderContext::alloc_surface_raw(wrapper, &mut buf);
                (*cbm).surface = s as *mut Surface;
            }

            // Lock the surface (slot 3): writes data ptr to `surf_data`
            // and stride to `surf_stride`.
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

            // Compute the source row pointer for this strip.
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
            // (other bpp: no blit, just unlock — matches original)

            // Unlock the surface (slot 4) with the locked data ptr.
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
    // dest_x: snap to a 0x40 grid by computing (dest_x mod 0x40) - 0x40
    // (with the mod result in [-0x3f, 0x3f] using sign-preserving
    // rounding). The result is the X offset of the FIRST visible tile
    // column relative to the screen origin: in the range [-0x3f, 0],
    // such that stepping by 0x40 produces tile-aligned column positions
    // that cover the visible area.
    let col_x: i32 = {
        // Reproduce the original's signed mod 0x40:
        //   AND EAX, 0x8000003f      ; preserve sign bit + low 6 bits
        //   if negative: DEC, OR 0xffffffc0, INC   (sign-extend)
        let dest_x_u = dest_x as u32;
        let masked = dest_x_u & 0x8000_003f;
        let mut v = if (masked as i32) < 0 {
            // sign-preserve
            (((masked.wrapping_sub(1)) | 0xffff_ffc0).wrapping_add(1)) as i32
        } else {
            masked as i32
        };
        // If v > 0, subtract 0x40.
        if v > 0 {
            v -= 0x40;
        }
        v
    };

    // Y-tile range: compute first/last visible tile index from
    //   (camera_y + dest_y) and (camera_y + dest_y - display_height)
    // using the same signed `(v + ((v >> 31) & 0x3FF)) >> 10` rounding
    // idiom that the original applies (round toward zero).
    let camera_y = (*this).camera_y;
    let neg = -(camera_y + dest_y);
    let display_height = (*this).base.display_height as i32;
    let first_v = neg + 0x20000;
    let last_v = display_height + neg + 0x20000;

    let mut y_first = (((first_v + ((first_v >> 31) & 0x3FF)) >> 10) - 0x80) as i32;
    let mut y_last = (((last_v + ((last_v >> 31) & 0x3FF)) >> 10) - 0x80) as i32;

    // Clamp y_first to >= 0 (matches the SETLE/SUB/AND idiom).
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
    let mut current_strip_y = y_first << 10; // y_first * 0x400

    while tile_idx_y <= y_last {
        let strip_end = total_height.min(current_strip_y + 0x400);
        let strip_h = strip_end - current_strip_y;

        // Skip the column loop if the start column is already off-screen.
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

/// Thiscall entry point for `DisplayGfx::DrawTiledBitmap` (vtable slot 11).
/// Matches the original's signature: `(this, dest_x, dest_y, source)` with
/// `RET 0xC` (3 stack args, callee-cleaned).
///
/// # Safety
/// See [`draw_tiled_bitmap_impl`].
pub unsafe extern "thiscall" fn draw_tiled_bitmap(
    this: *mut DisplayGfx,
    dest_x: i32,
    dest_y: i32,
    source: *const TiledBitmapSource,
) {
    draw_tiled_bitmap_impl(this, dest_x, dest_y, source);
}
