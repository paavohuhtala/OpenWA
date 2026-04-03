/// DDDisplay — display/rendering subsystem.
///
/// Constructor: DDDisplay__Init (0x569D00).
/// Vtable: 0x66A218 (38 known slots).
/// Destructor: FUN_00569CE0.
///
/// Manages display mode, dimensions, palette, and HWND.
/// Contains DrawTextOnBitmap (thiscall) and ConstructTextbox methods.
/// The OpenGL context (HDC, HGLRC) is also stored here when GL mode is active.
///
/// OPAQUE: Full struct size not yet determined. Only vtable pointer defined.
#[repr(C)]
pub struct DDDisplay {
    /// 0x000: Vtable pointer (0x66A218)
    pub vtable: *const DDDisplayVtable,
}

/// DDDisplay vtable (0x66A218, 38 slots).
///
/// Only actively-used slots have typed signatures. Unknown slots are auto-filled.
#[openwa_core::vtable(size = 38, va = 0x0066_A218, class = "DDDisplay")]
pub struct DDDisplayVtable {
    /// set layer color
    #[slot(4)]
    pub set_layer_color: fn(this: *mut DDDisplay, layer: i32, color: i32),
    /// set active layer, returns layer context ptr
    #[slot(5)]
    pub set_active_layer: fn(this: *mut DDDisplay, layer: i32) -> *mut u8,
    /// set layer visibility
    #[slot(23)]
    pub set_layer_visibility: fn(this: *mut DDDisplay, layer: i32, value: i32),
    /// load .fnt bitmap font into a font slot (RET 0x10)
    ///
    /// DDDisplay__LoadFont (0x523560).
    /// mode: font source (1-3, usually 1). font_id: slot index (1-based, max 0x1F).
    /// gfx: GfxDir archive to load from. filename: .fnt resource name.
    #[slot(34)]
    pub load_font: fn(
        this: *mut DDDisplay,
        mode: i32,
        font_id: i32,
        gfx: *mut u8,
        filename: *const core::ffi::c_char,
    ) -> u32,
    /// load .fex font extension (extra glyphs) for a font slot (RET 0x14)
    ///
    /// DDDisplay__LoadFontExtension (0x523620).
    /// font_id: slot index. path: filesystem path to .fex file.
    /// char_map: 61-byte character code table. palette_value: color from GfxColorTable.
    /// flag: unknown (always 0 from DDGameWrapper__LoadFontExtension).
    #[slot(35)]
    pub load_font_extension: fn(
        this: *mut DDDisplay,
        font_id: i32,
        path: *const core::ffi::c_char,
        char_map: *const u8,
        palette_value: u32,
        flag: i32,
    ) -> u32,
    /// set font palette for all loaded fonts (RET 0x8)
    ///
    /// DDDisplay__SetFontPalette (0x523690).
    /// font_count: total number of font slots loaded.
    /// palette_value: palette/color entry from DDGame GfxColorTable.
    #[slot(36)]
    pub set_font_palette: fn(this: *mut DDDisplay, font_count: u32, palette_value: u32),
    /// load sprite with flag (RET 0x14)
    #[slot(31)]
    pub load_sprite: fn(
        this: *mut DDDisplay,
        layer: u32,
        id: u32,
        flag: u32,
        gfx: *mut u8,
        name: *const core::ffi::c_char,
    ) -> i32,
    /// load sprite by layer (RET 0x10)
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
