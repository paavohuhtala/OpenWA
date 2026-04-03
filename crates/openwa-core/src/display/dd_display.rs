use crate::fixed::Fixed;

/// DDDisplay — display/rendering subsystem.
///
/// Constructor: DDDisplay__Init (0x569D00).
/// Vtable: 0x66A218 (38 slots).
/// Destructor: 0x569CE0.
///
/// Actual runtime type is DisplayGfx (0x24E28 bytes), which extends DisplayBase.
/// Manages layers, sprites, fonts, palettes, and delegates to a renderer backend
/// (CompatRenderer for D3D/DDraw, OpenGLCPU for OpenGL).
///
/// Key internal fields (offsets from DisplayBase):
/// - 0x3548/0x354C: display width/height
/// - 0x3550-0x355C: clip rect (x1, y1, x2, y2)
/// - 0x3560/0x3564: camera offset (x, y)
/// - 0x3580-0x3584: bitmap vector (ptr, end)
/// - 0x3D9C: renderer backend pointer
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
