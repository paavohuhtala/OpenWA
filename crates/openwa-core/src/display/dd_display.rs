use crate::fixed::Fixed;

/// DDDisplay — display/rendering subsystem.
///
/// Constructor: DDDisplay__Init (0x569D00).
/// Vtable: 0x66A218 (38 slots).
/// Destructor: 0x569CE0.
///
/// Actual runtime type is DisplayGfx (0x24E28 bytes), which extends DisplayBase.
/// Manages layers, sprites, fonts, palettes, and delegates rendering through
/// DDDisplayWrapper (g_DDDisplayWrapper at 0x79D6D4), which in turn dispatches
/// to a renderer backend (CompatRenderer for D3D/DDraw, OpenGLCPU for OpenGL).
///
/// ## Dispatch chain
///
/// ```text
/// DDDisplay  →  DDDisplayWrapper  →  RendererBackend
///   vtable        (CWormsApp sub-      (CompatRenderer/
///   0x66A218       object, vtable       OpenGLCPU/DDraw)
///                  0x662EC8)
/// ```
///
/// DDDisplay methods apply camera offset and clipping, then call through
/// `g_DDDisplayWrapper->vtable[N]` for the actual rendering operation.
///
/// Key internal fields (offsets from start of DisplayGfx/DisplayBase):
/// - 0x3548/0x354C: display width/height (pixels)
/// - 0x3550-0x355C: clip rect (x1, y1, x2, y2, pixels)
/// - 0x3560/0x3564: camera offset (x, y, pixels)
/// - 0x3580-0x3584: bitmap vector (ptr, end)
/// - 0x3D98: render lock flag (cleared by FlushRender)
/// - 0x3D9C: display context pointer (read during FlushRender)
///
/// OPAQUE: Full struct layout not yet mapped (see DisplayGfx for size).
#[repr(C)]
pub struct DDDisplay {
    /// 0x000: Vtable pointer (0x66A218)
    pub vtable: *const DDDisplayVtable,
}

/// DDDisplay vtable (0x66A218, 38 slots).
///
/// Slots 2, 3, 25, 29 are stubs (CGameTask no-ops).
/// All other slots are standard thiscall.
///
/// Coordinate conventions:
/// - Methods that add `camera * 0x10000` take Fixed (16.16) coordinates.
/// - Methods that add `camera` directly take pixel-integer coordinates.
#[openwa_core::vtable(size = 38, va = 0x0066_A218, class = "DDDisplay")]
pub struct DDDisplayVtable {
    /// destructor (0x569CE0, RET 0x4)
    #[slot(0)]
    pub destructor: fn(this: *mut DDDisplay, flags: u8) -> *mut DDDisplay,
    /// get display dimensions in pixels (0x56A460, RET 0x8)
    #[slot(1)]
    pub get_dimensions: fn(this: *mut DDDisplay, out_w: *mut u32, out_h: *mut u32),
    /// set layer color (0x5231E0, RET 0x8)
    #[slot(4)]
    pub set_layer_color: fn(this: *mut DDDisplay, layer: i32, color: i32),
    /// set active layer, returns layer context ptr (0x523270, RET 0x4)
    #[slot(5)]
    pub set_active_layer: fn(this: *mut DDDisplay, layer: i32) -> *mut u8,
    /// get sprite info by layer and id (0x523500, RET 0x10)
    #[slot(6)]
    pub get_sprite_info: fn(this: *mut DDDisplay, layer: i32, p3: u32, p4: u32) -> u32,
    /// draw text onto a bitmap surface (0x5236B0, RET 0x1C)
    ///
    /// font_id low 16 bits = font slot (1-based), high 16 bits = extra flags.
    #[slot(7)]
    pub draw_text_on_bitmap: fn(
        this: *mut DDDisplay,
        font_id: i32,
        bitmap: i32,
        h_align: i32,
        v_align: i32,
        msg: *const core::ffi::c_char,
        a7: i32,
        a8: i32,
    ) -> i32,
    /// get font info for a font slot (0x523790, RET 0xC)
    #[slot(8)]
    pub get_font_info: fn(this: *mut DDDisplay, font_id: i32, p3: u32) -> u32,
    /// get font metric (0x523750, RET 0x10)
    #[slot(9)]
    pub get_font_metric: fn(this: *mut DDDisplay, font_id: i32, p3: u32, p4: u32) -> u32,
    /// set font rendering parameter (0x523710, RET 0x10)
    #[slot(10)]
    pub set_font_param: fn(this: *mut DDDisplay, font_id: i32, p3: u32, p4: u32, p5: u32) -> u32,
    /// create bitmap object (0x56B8C0, RET 0xC)
    ///
    /// Allocates CBitmap, initializes from sprite data.
    #[slot(11)]
    pub create_bitmap: fn(this: *mut DDDisplay, count: u32, p3: i32, p4: i32),
    /// draw polyline with camera offset (0x56BCC0, RET 0xC)
    ///
    /// Transforms point array by camera offset, then draws connected line segments.
    /// Points are pixel-integer pairs.
    #[slot(12)]
    pub draw_polyline: fn(this: *mut DDDisplay, points: *mut i32, count: i32, color: u32),
    /// draw line with camera offset (0x56BDB0, RET 0x18)
    ///
    /// Coordinates are fixed-point; camera is applied as `camera * 0x10000 + coord`.
    #[slot(13)]
    pub draw_line: fn(
        this: *mut DDDisplay,
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
        fn(this: *mut DDDisplay, x1: Fixed, y1: Fixed, x2: Fixed, y2: Fixed, color: u32),
    /// draw repeated pixels along a vector (0x56BE10, RET 0x18)
    ///
    /// Draws count+1 pixels starting at (x,y), stepping by (dx,dy) each iteration.
    /// All coordinates are fixed-point.
    #[slot(15)]
    pub draw_pixel_strip:
        fn(this: *mut DDDisplay, x: Fixed, y: Fixed, dx: Fixed, dy: Fixed, count: i32, color: u32),
    /// draw crosshair pattern — 9 pixels in a cross (0x56BE80, RET 0x10)
    ///
    /// Pixel-integer coordinates (adds camera directly).
    #[slot(16)]
    pub draw_crosshair: fn(this: *mut DDDisplay, x: i32, y: i32, color_fg: u32, color_bg: u32),
    /// draw outlined pixel — center + 4 cardinal neighbors (0x56BFD0, RET 0x10)
    ///
    /// Pixel-integer coordinates.
    #[slot(17)]
    pub draw_outlined_pixel: fn(this: *mut DDDisplay, x: i32, y: i32, color_fg: u32, color_bg: i32),
    /// fill rectangle with camera offset and clipping (0x56B810, RET 0x14)
    ///
    /// Pixel-integer coordinates (adds camera directly, then clips).
    #[slot(18)]
    pub fill_rect: fn(this: *mut DDDisplay, x1: i32, y1: i32, x2: i32, y2: i32, color: u32),
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
    pub blit_sprite: fn(this: *mut DDDisplay, x: Fixed, y: Fixed, sprite_flags: u32, palette: u32),
    /// draw scaled/rotated sprite (0x56B660, RET 0x20)
    ///
    /// x/y are fixed-point world coordinates.
    /// Dispatches by flags: bit 21 mirror, bit 26 additive, bit 27/28 blend modes.
    #[slot(20)]
    pub draw_scaled_sprite: fn(
        this: *mut DDDisplay,
        x: Fixed,
        y: Fixed,
        sprite: u32,
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
        fn(this: *mut DDDisplay, x: Fixed, y: Fixed, obj: *mut u8, p5: u32, p6: u32),
    /// stream/animation data to display (0x56C5A0, RET 0x10)
    #[slot(22)]
    pub stream_data: fn(this: *mut DDDisplay, p2: i32, p3: i32, count: i32, flags: u32),
    /// set layer visibility (0x56A5D0, RET 0x8)
    #[slot(23)]
    pub set_layer_visibility: fn(this: *mut DDDisplay, layer: i32, visible: i32),
    /// update palette from palette data (0x56A610, RET 0x8)
    #[slot(24)]
    pub update_palette: fn(this: *mut DDDisplay, palette_data: *mut i16, p3: i32),
    // Slot 25: stub (CGameTask__vt19)
    /// flush pending render state (0x56A580, plain RET)
    ///
    /// No stack params. Releases renderer lock and
    /// calls through DDDisplayWrapper vtable to finalize.
    #[slot(26)]
    pub flush_render: fn(this: *mut DDDisplay),
    /// set camera offset (0x56CC40, RET 0x8)
    ///
    /// Fixed-point input; internally `>> 16` to pixel integers stored at +0x3560/+0x3564.
    #[slot(27)]
    pub set_camera_offset: fn(this: *mut DDDisplay, x: Fixed, y: Fixed),
    /// set clip rectangle (0x56CC60, RET 0x10)
    ///
    /// Fixed-point input; internally `>> 16` to pixel integers, clamped to display dimensions.
    #[slot(28)]
    pub set_clip_rect: fn(this: *mut DDDisplay, x1: Fixed, y1: Fixed, x2: Fixed, y2: Fixed),
    // Slot 29: stub (CGameTask__vt18)
    /// load sprite with extended params (0x523310, RET 0x18)
    ///
    /// Allocates 0x17C sprite object, calls DisplayGfx constructor.
    #[slot(30)]
    pub load_sprite_ex:
        fn(this: *mut DDDisplay, mode: i32, id: i32, p4: u32, count: i32, p6: u32, p7: u32) -> i32,
    /// load sprite into layer (0x523400, RET 0x14)
    #[slot(31)]
    pub load_sprite: fn(
        this: *mut DDDisplay,
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
    pub is_sprite_loaded: fn(this: *mut DDDisplay, id: i32) -> u32,
    /// load sprite with complex params (0x5237C0, RET 0x24)
    ///
    /// 9 stack params. Dual-path loading depending on layer type.
    #[slot(33)]
    pub load_sprite_complex: fn(
        this: *mut DDDisplay,
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
        this: *mut DDDisplay,
        mode: i32,
        font_id: i32,
        gfx: *mut u8,
        filename: *const core::ffi::c_char,
    ) -> u32,
    /// load .fex font extension for a font slot (0x523620, RET 0x14)
    #[slot(35)]
    pub load_font_extension: fn(
        this: *mut DDDisplay,
        font_id: i32,
        path: *const core::ffi::c_char,
        char_map: *const u8,
        palette_value: u32,
        flag: i32,
    ) -> u32,
    /// set font palette for all loaded fonts (0x523690, RET 0x8)
    #[slot(36)]
    pub set_font_palette: fn(this: *mut DDDisplay, font_count: u32, palette_value: u32),
    /// load sprite by layer with fallback allocation (0x56A4C0, RET 0x10)
    #[slot(37)]
    pub load_sprite_by_layer: fn(
        this: *mut DDDisplay,
        layer: u32,
        id: u32,
        gfx: *mut u8,
        name: *const core::ffi::c_char,
    ) -> i32,
}

// Generate calling wrappers: DDDisplay::set_layer_color(), etc.
bind_DDDisplayVtable!(DDDisplay, vtable);

// =========================================================================
// Ported DDDisplay vtable methods
// =========================================================================

use super::bitgrid::DisplayBitGrid;
use super::gfx::DisplayGfx;
use super::line_draw;

/// Port of DDDisplay::GetDimensions (vtable slot 1, 0x56A460).
///
/// Reads display_width/display_height from DisplayBase and writes them
/// to the output pointers.
///
/// # Safety
/// `this` must be a valid `*mut DDDisplay` (actually a `*mut DisplayGfx`).
pub unsafe extern "thiscall" fn get_dimensions(
    this: *mut DDDisplay,
    out_w: *mut u32,
    out_h: *mut u32,
) {
    let gfx = this as *mut DisplayGfx;
    *out_w = (*gfx).base.display_width;
    *out_h = (*gfx).base.display_height;
}

use super::display_wrapper::{DDDisplayWrapper, FastcallResult};
use crate::address::va;
use crate::rebase::rb;

/// Port of DDDisplay::FlushRender (vtable slot 26, 0x56A580).
///
/// If the render lock is held, clears it (the original also calls
/// DDDisplayWrapper::unlock_surface_write, but that's a no-op that
/// writes a success code to a discarded result buffer).
///
/// Then calls DDDisplayWrapper::get_renderer_surface (slot 13),
/// which dispatches to the renderer backend's Flip.
///
/// # Safety
/// `this` must be a valid `*mut DDDisplay` (actually a `*mut DisplayGfx`).
/// `g_DDDisplayWrapper` (0x79D6D4) must be initialized.
pub unsafe extern "thiscall" fn flush_render(this: *mut DDDisplay) {
    let gfx = this as *mut DisplayGfx;
    let wrapper = *(rb(va::G_DD_DISPLAY_WRAPPER) as *const *mut DDDisplayWrapper);

    if (*gfx).render_lock != 0 {
        // Original calls wrapper->vtable[18] (unlock_surface_write) here,
        // but that function ignores its data parameter and just writes a
        // success code to a result buffer that FlushRender never reads.
        (*gfx).render_lock = 0;
    }

    // get_renderer_surface → renderer Flip
    let mut buf = FastcallResult::default();
    DDDisplayWrapper::get_renderer_surface_raw(wrapper, &mut buf);
}

/// Port of DDDisplay::SetCameraOffset (vtable slot 27, 0x56CC40).
///
/// Converts Fixed-point camera coordinates to pixel integers and stores them.
///
/// # Safety
/// `this` must be a valid `*mut DDDisplay` (actually a `*mut DisplayGfx`).
pub unsafe extern "thiscall" fn set_camera_offset(this: *mut DDDisplay, x: Fixed, y: Fixed) {
    let gfx = this as *mut DisplayGfx;
    (*gfx).camera_x = x.to_int();
    (*gfx).camera_y = y.to_int();
}

/// Port of DDDisplay::SetClipRect (vtable slot 28, 0x56CC60).
///
/// Converts Fixed-point clip rectangle to pixel integers, clamps to display
/// dimensions, stores in DisplayBase, and mirrors to the layer_0 object.
///
/// # Safety
/// `this` must be a valid `*mut DDDisplay` (actually a `*mut DisplayGfx`).
/// `layer_0` (at +0x3D9C) must be initialized.
pub unsafe extern "thiscall" fn set_clip_rect(
    this: *mut DDDisplay,
    x1: Fixed,
    y1: Fixed,
    x2: Fixed,
    y2: Fixed,
) {
    let gfx = this as *mut DisplayGfx;
    let base = &mut (*gfx).base;

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
    let layer = (*gfx).layer_0;
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
/// DDDisplayWrapper::unlock_surface_write (slot 18), then clears the flag.
/// Called by drawing methods (fill_rect, draw_line, etc.) before rendering.
///
/// Original uses ESI = this (usercall).
///
/// # Safety
/// `gfx` must be a valid `*mut DisplayGfx` with `layer_0` initialized.
/// `g_DDDisplayWrapper` must be initialized.
unsafe fn flush_render_lock(gfx: *mut DisplayGfx) {
    if (*gfx).render_lock != 0 {
        let wrapper = *(rb(va::G_DD_DISPLAY_WRAPPER) as *const *mut DDDisplayWrapper);
        let data = (*(*gfx).layer_0).data as u32;
        let mut buf = FastcallResult::default();
        DDDisplayWrapper::unlock_surface_write_raw(wrapper, &mut buf, data);
        (*gfx).render_lock = 0;
    }
}

/// Port of the render-lock acquire helper at 0x56A370.
///
/// If the render lock is NOT held, queries DDDisplayWrapper for framebuffer
/// dimensions (slot 3) and locks the surface for writing (slot 17), then
/// populates layer_0's BitGrid fields (data, stride, dimensions) from the
/// locked surface and copies DisplayBase's clip rect into layer_0.
///
/// Original uses ESI = this (usercall).
///
/// # Safety
/// `gfx` must be a valid `*mut DisplayGfx` with `layer_0` initialized.
/// `g_DDDisplayWrapper` must be initialized.
unsafe fn acquire_render_lock(gfx: *mut DisplayGfx) {
    if (*gfx).render_lock != 0 {
        return; // already locked
    }

    let wrapper = *(rb(va::G_DD_DISPLAY_WRAPPER) as *const *mut DDDisplayWrapper);
    let mut buf = FastcallResult::default();

    // Get framebuffer dimensions (slot 3).
    // The wrapper writes width and height to the output buffer.
    let mut dims: [u32; 2] = [0; 2];
    DDDisplayWrapper::get_framebuffer_dims_raw(wrapper, &mut buf, dims.as_mut_ptr());
    let fb_width = dims[0];
    let fb_height = dims[1];

    // Lock surface for writing (slot 17).
    // The wrapper writes framebuffer pointer and stride through the params.
    let mut data_ptr: u32 = 0;
    let mut stride: u32 = 0;
    DDDisplayWrapper::lock_surface_write_raw(wrapper, &mut buf, &mut data_ptr, &mut stride);

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

/// Port of DDDisplay::FillRect (vtable slot 18, 0x56B810).
///
/// Fills a rectangle with a solid color. Pixel-integer coordinates are
/// offset by the camera position and clipped to the clip rect before
/// dispatching to DDDisplayWrapper::fill_rect (slot 19).
///
/// # Safety
/// `this` must be a valid `*mut DDDisplay` (actually a `*mut DisplayGfx`).
/// `g_DDDisplayWrapper` must be initialized.
pub unsafe extern "thiscall" fn fill_rect(
    this: *mut DDDisplay,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    color: u32,
) {
    let gfx = this as *mut DisplayGfx;
    let base = &(*gfx).base;

    // Apply camera offset
    let mut left = x1 + (*gfx).camera_x;
    let mut top = y1 + (*gfx).camera_y;
    let mut right = x2 + (*gfx).camera_x;
    let mut bottom = y2 + (*gfx).camera_y;

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

    flush_render_lock(gfx);

    // DDDisplayWrapper::fill_rect takes (x, y, width, height, color)
    let wrapper = *(rb(va::G_DD_DISPLAY_WRAPPER) as *const *mut DDDisplayWrapper);
    let mut buf = FastcallResult::default();
    DDDisplayWrapper::fill_rect_raw(
        wrapper,
        &mut buf,
        left,
        top,
        right - left,
        bottom - top,
        color,
    );
}

/// Port of DDDisplay::DrawOutlinedPixel (vtable slot 17, 0x56BFD0).
///
/// Draws a center pixel in `color_fg` with 4 cardinal neighbor pixels in
/// `color_bg` (if `color_bg != 0`). Pixel-integer coordinates with camera offset.
///
/// # Safety
/// `this` must be a valid `*mut DDDisplay` (actually a `*mut DisplayGfx`).
pub unsafe extern "thiscall" fn draw_outlined_pixel(
    this: *mut DDDisplay,
    x: i32,
    y: i32,
    color_fg: u32,
    color_bg: i32,
) {
    let gfx = this as *mut DisplayGfx;
    let cx = x + (*gfx).camera_x;
    let cy = y + (*gfx).camera_y;

    acquire_render_lock(gfx);

    let layer = (*gfx).layer_0;
    if color_bg != 0 {
        let bg = color_bg as u8;
        DisplayBitGrid::put_pixel_clipped_raw(layer, cx - 1, cy, bg);
        DisplayBitGrid::put_pixel_clipped_raw(layer, cx + 1, cy, bg);
        DisplayBitGrid::put_pixel_clipped_raw(layer, cx, cy - 1, bg);
        DisplayBitGrid::put_pixel_clipped_raw(layer, cx, cy + 1, bg);
    }
    DisplayBitGrid::put_pixel_clipped_raw(layer, cx, cy, color_fg as u8);
}

/// Port of DDDisplay::DrawCrosshair (vtable slot 16, 0x56BE80).
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
/// `this` must be a valid `*mut DDDisplay` (actually a `*mut DisplayGfx`).
pub unsafe extern "thiscall" fn draw_crosshair(
    this: *mut DDDisplay,
    x: i32,
    y: i32,
    color_fg: u32,
    color_bg: u32,
) {
    let gfx = this as *mut DisplayGfx;
    let cx = x + (*gfx).camera_x;
    let cy = y + (*gfx).camera_y;

    acquire_render_lock(gfx);

    let layer = (*gfx).layer_0;
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

/// Port of DDDisplay::DrawLine (vtable slot 13, 0x56BDB0).
///
/// Draws a two-color thick line. Fixed-point coordinates with camera offset
/// applied as `camera * 0x10000 + coord`. Pure Rust implementation.
///
/// # Safety
/// `this` must be a valid `*mut DDDisplay` (actually a `*mut DisplayGfx`).
pub unsafe extern "thiscall" fn draw_line(
    this: *mut DDDisplay,
    x1: Fixed,
    y1: Fixed,
    x2: Fixed,
    y2: Fixed,
    color1: u32,
    color2: u32,
) {
    let gfx = this as *mut DisplayGfx;
    let cam_x = Fixed::from_int((*gfx).camera_x);
    let cam_y = Fixed::from_int((*gfx).camera_y);

    acquire_render_lock(gfx);

    let mut writer = BitGridWriter((*gfx).layer_0);
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

/// Port of DDDisplay::DrawLineClipped (vtable slot 14, 0x56BD50).
///
/// Draws a single-color clipped line. Fixed-point coordinates with camera
/// offset. Pure Rust implementation.
///
/// # Safety
/// `this` must be a valid `*mut DDDisplay` (actually a `*mut DisplayGfx`).
pub unsafe extern "thiscall" fn draw_line_clipped(
    this: *mut DDDisplay,
    x1: Fixed,
    y1: Fixed,
    x2: Fixed,
    y2: Fixed,
    color: u32,
) {
    let gfx = this as *mut DisplayGfx;
    let cam_x = Fixed::from_int((*gfx).camera_x);
    let cam_y = Fixed::from_int((*gfx).camera_y);

    acquire_render_lock(gfx);

    let mut writer = BitGridWriter((*gfx).layer_0);
    line_draw::draw_line_clipped(
        &mut writer,
        x1 + cam_x,
        y1 + cam_y,
        x2 + cam_x,
        y2 + cam_y,
        color as u8,
    );
}

/// Port of DDDisplay::DrawPixelStrip (vtable slot 15, 0x56BE10).
///
/// Draws `count + 1` pixels starting at (x, y), stepping by (dx, dy) each
/// iteration. All coordinates are Fixed-point. Camera applied as
/// `camera * 0x10000 + coord`.
///
/// # Safety
/// `this` must be a valid `*mut DDDisplay` (actually a `*mut DisplayGfx`).
pub unsafe extern "thiscall" fn draw_pixel_strip(
    this: *mut DDDisplay,
    x: Fixed,
    y: Fixed,
    dx: Fixed,
    dy: Fixed,
    count: i32,
    color: u32,
) {
    let gfx = this as *mut DisplayGfx;
    let mut cx = Fixed::from_int((*gfx).camera_x) + x;
    let mut cy = Fixed::from_int((*gfx).camera_y) + y;

    acquire_render_lock(gfx);

    let layer = (*gfx).layer_0;
    if count >= 0 {
        for _ in 0..=count {
            DisplayBitGrid::put_pixel_clipped_raw(layer, cx.to_int(), cy.to_int(), color as u8);
            cx += dx;
            cy += dy;
        }
    }
}
