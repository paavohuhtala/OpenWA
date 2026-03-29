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
