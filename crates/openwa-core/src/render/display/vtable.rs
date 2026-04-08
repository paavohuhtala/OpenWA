use crate::fixed::Fixed;
use crate::render::display::line_draw::Vertex;

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
#[openwa_core::vtable(size = 38, va = 0x0066_A218, class = "DisplayGfx")]
pub struct DisplayVtable {
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
    /// draw text onto a bitmap surface (0x5236B0, RET 0x1C)
    ///
    /// font_id low 16 bits = font slot (1-based), high 16 bits = extra flags.
    #[slot(7)]
    pub draw_text_on_bitmap: fn(
        this: *mut DisplayGfx,
        font_id: i32,
        bitmap: i32,
        h_align: i32,
        v_align: i32,
        msg: *const core::ffi::c_char,
        a7: i32,
        a8: i32,
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
    /// create bitmap object (0x56B8C0, RET 0xC)
    ///
    /// Allocates CBitmap, initializes from sprite data.
    #[slot(11)]
    pub create_bitmap: fn(this: *mut DisplayGfx, count: u32, p3: i32, p4: i32),
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
    pub update_palette: fn(
        this: *mut DisplayGfx,
        palette_ctx: *mut crate::render::palette::PaletteContext,
        commit: i32,
    ),
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
        gfx: *mut u8,
        name: *const core::ffi::c_char,
    ) -> i32,
    /// check if a sprite ID is loaded (0x56A480, RET 0x4)
    ///
    /// Returns 1 if the sprite exists in any of the three sprite arrays.
    #[slot(32)]
    pub is_sprite_loaded: fn(this: *mut DisplayGfx, id: i32) -> u32,
    /// load sprite with complex params (0x5237C0, RET 0x24)
    ///
    /// 9 stack params. Dual-path loading depending on layer type.
    #[slot(33)]
    pub load_sprite_complex: fn(
        this: *mut DisplayGfx,
        layer: i32,
        p3: u32,
        p4: u32,
        p5: u32,
        p6: u32,
        p7: u32,
        p8: u32,
        p9: u32,
        p10: u32,
    ) -> u32,
    /// load .fnt bitmap font into a font slot (0x523560, RET 0x10)
    #[slot(34)]
    pub load_font: fn(
        this: *mut DisplayGfx,
        mode: i32,
        font_id: i32,
        gfx: *mut u8,
        filename: *const core::ffi::c_char,
    ) -> u32,
    /// load .fex font extension for a font slot (0x523620, RET 0x14)
    #[slot(35)]
    pub load_font_extension: fn(
        this: *mut DisplayGfx,
        font_id: i32,
        path: *const core::ffi::c_char,
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
        gfx: *mut u8,
        name: *const core::ffi::c_char,
    ) -> i32,
}

// Generate calling wrappers: DisplayGfx::set_layer_color(), etc.
bind_DisplayVtable!(DisplayGfx, base.vtable);

// =========================================================================
// Ported DisplayGfx vtable methods
// =========================================================================

use super::base::DisplayBase;
use super::gfx::DisplayGfx;
use super::line_draw;
use crate::bitgrid::DisplayBitGrid;
use crate::render::palette::PaletteContext;
use crate::render::sprite::{Sprite, SpriteBank};

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
    let id_u = id as u32;
    if id_u.wrapping_sub(1) >= 0x3FF {
        return 0;
    }

    let base = &(*this).base;
    if !base.sprite_ptrs[id as usize].is_null()
        || !base.sprite_banks[id as usize].is_null()
        || (*this).sprite_table[id as usize] != 0
    {
        1
    } else {
        0
    }
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

            blit_bitmap_clipped(
                this,
                col_w,
                pixel_x + x_offset,
                pixel_y + y_offset,
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

/// Bridge to DisplayGfx__BlitBitmapClipped (0x56A700).
///
/// Usercall: EAX=this (DisplayGfx*), EDX=col_width, 5 stack params (dst_x, dst_y,
/// row_height, bitmap_ptr, flags), RET 0x14.
///
/// Clips the bitmap rectangle against the display clip rect, calls flush_render_lock,
/// then delegates to the low-level blit function (0x403C60).
unsafe fn blit_bitmap_clipped(
    gfx: *mut DisplayGfx,
    col_width: i32,
    dst_x: i32,
    dst_y: i32,
    row_height: i32,
    bitmap_ptr: u32,
    flags: u32,
) {
    blit_bitmap_clipped_bridge(
        gfx as u32,
        col_width as u32,
        dst_x as u32,
        dst_y as u32,
        row_height as u32,
        bitmap_ptr,
        flags,
        rb(va::DISPLAY_GFX_BLIT_BITMAP_CLIPPED),
    );
}

/// Naked bridge: sets EAX=this, EDX=col_width, pushes 5 stack params, calls target.
#[unsafe(naked)]
unsafe extern "cdecl" fn blit_bitmap_clipped_bridge(
    _this: u32,
    _col_width: u32,
    _dst_x: u32,
    _dst_y: u32,
    _row_height: u32,
    _bitmap_ptr: u32,
    _flags: u32,
    _target: u32,
) {
    core::arch::naked_asm!(
        "mov eax, [esp + 4]", // EAX = this
        "mov edx, [esp + 8]", // EDX = col_width
        "push [esp + 28]",    // flags
        "push [esp + 28]",    // bitmap_ptr (shifted by our push)
        "push [esp + 28]",    // row_height
        "push [esp + 28]",    // dst_y
        "push [esp + 28]",    // dst_x
        "call [esp + 52]",    // target (offset: 5 pushes × 4 + 32 original)
        "ret",
    );
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
    // Layer must be 1, 2, or 3
    if (layer as u32).wrapping_sub(1) >= 3 {
        return;
    }

    // Only allocate if no context exists for this layer
    if (*this).base.layer_contexts[layer as usize] != 0 {
        return;
    }

    // Find `color` consecutive available (non-zero) entries in the slot table area.
    // The scan covers slot_table_guard (guard=0) + slot_table[0..255] + slot_table_sentinel (sentinel=-1).
    let start = palette_slot_alloc(&mut (*this).base, color);

    // Allocate PaletteContext (0x72C total, zero first 0x70C)
    let ctx = crate::wa_alloc::wa_malloc(0x72C);
    core::ptr::write_bytes(ctx, 0, 0x70C);

    let result = if ctx.is_null() {
        0u32
    } else {
        let ctx = ctx as *mut PaletteContext;
        palette_context_init(ctx, start as i16, (start as i32 + color - 1) as i16);
        ctx as u32
    };

    (*this).base.layer_contexts[layer as usize] = result;
    (*this).base.layer_visibility[layer as usize] = 0;
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
unsafe fn palette_slot_alloc(base: &mut DisplayBase<*const DisplayVtable>, count: i32) -> i32 {
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
    if (layer as u32).wrapping_sub(1) < 3 {
        (*this).base.layer_contexts[layer as usize] as *mut u8
    } else {
        core::ptr::null_mut()
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
    let layer_ctx = set_active_layer(this, layer) as *mut PaletteContext;
    if !layer_ctx.is_null() {
        update_palette(this, layer_ctx, visible);
    }

    if visible < 0 {
        (*this).base.layer_visibility[layer as usize] = 0;
    }
}
