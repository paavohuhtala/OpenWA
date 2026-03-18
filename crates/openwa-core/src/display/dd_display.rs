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
/// Only actively-used slots have typed signatures. Unknown slots are `usize`.
/// Follows the DSSoundVtable pattern: all slots defined, unknowns as stubs.
#[repr(C)]
pub struct DDDisplayVtable {
    /// Slot 0: destructor
    pub _slot_00: usize,
    /// Slot 1
    pub _slot_01: usize,
    /// Slot 2
    pub _slot_02: usize,
    /// Slot 3
    pub _slot_03: usize,
    /// Slot 4: set layer color — thiscall(this, layer, color)
    pub set_layer_color: unsafe extern "thiscall" fn(*mut DDDisplay, i32, i32),
    /// Slot 5: set active layer — thiscall(this, layer) -> layer context ptr
    pub set_active_layer: unsafe extern "thiscall" fn(*mut DDDisplay, i32) -> *mut u8,
    /// Slot 6
    pub _slot_06: usize,
    /// Slot 7
    pub _slot_07: usize,
    /// Slot 8
    pub _slot_08: usize,
    /// Slot 9
    pub _slot_09: usize,
    /// Slot 10
    pub _slot_10: usize,
    /// Slot 11
    pub _slot_11: usize,
    /// Slot 12
    pub _slot_12: usize,
    /// Slot 13
    pub _slot_13: usize,
    /// Slot 14
    pub _slot_14: usize,
    /// Slot 15
    pub _slot_15: usize,
    /// Slot 16
    pub _slot_16: usize,
    /// Slot 17
    pub _slot_17: usize,
    /// Slot 18
    pub _slot_18: usize,
    /// Slot 19
    pub _slot_19: usize,
    /// Slot 20
    pub _slot_20: usize,
    /// Slot 21
    pub _slot_21: usize,
    /// Slot 22
    pub _slot_22: usize,
    /// Slot 23 (0x5C): set layer visibility — thiscall(this, layer, value)
    pub set_layer_visibility: unsafe extern "thiscall" fn(*mut DDDisplay, i32, i32),
    /// Slot 24
    pub _slot_24: usize,
    /// Slot 25
    pub _slot_25: usize,
    /// Slot 26
    pub _slot_26: usize,
    /// Slot 27
    pub _slot_27: usize,
    /// Slot 28
    pub _slot_28: usize,
    /// Slot 29
    pub _slot_29: usize,
    /// Slot 30
    pub _slot_30: usize,
    /// Slot 31 (0x7C): load sprite with flag — thiscall(this, layer, id, flag, gfx, name), RET 0x14
    pub load_sprite:
        unsafe extern "thiscall" fn(*mut DDDisplay, u32, u32, u32, *mut u8, *const u8) -> i32,
    /// Slot 32
    pub _slot_32: usize,
    /// Slot 33
    pub _slot_33: usize,
    /// Slot 34
    pub _slot_34: usize,
    /// Slot 35
    pub _slot_35: usize,
    /// Slot 36
    pub _slot_36: usize,
    /// Slot 37 (0x94): load sprite by layer — thiscall(this, layer, id, gfx, name), RET 0x10
    pub load_sprite_by_layer:
        unsafe extern "thiscall" fn(*mut DDDisplay, u32, u32, *mut u8, *const u8) -> i32,
}

const _: () = assert!(core::mem::size_of::<DDDisplayVtable>() == 38 * 4);

/// Typed wrappers for DDDisplay vtable calls.
/// All take `*mut DDDisplay` since thiscall passes `this` as mutable.
impl DDDisplay {
    /// Vtable slot 5: set active display layer, returns layer context pointer.
    #[inline]
    pub unsafe fn set_active_layer(this: *mut Self, layer: i32) -> *mut u8 {
        ((*(*this).vtable).set_active_layer)(this, layer)
    }

    /// Vtable slot 4: set color for a display layer.
    #[inline]
    pub unsafe fn set_layer_color(this: *mut Self, layer: i32, color: i32) {
        ((*(*this).vtable).set_layer_color)(this, layer, color)
    }

    /// Vtable slot 23: set layer visibility.
    #[inline]
    pub unsafe fn set_layer_visibility(this: *mut Self, layer: i32, value: i32) {
        ((*(*this).vtable).set_layer_visibility)(this, layer, value)
    }

    /// Vtable slot 37: load sprite into a layer by name.
    #[inline]
    pub unsafe fn load_sprite_by_layer(
        this: *mut Self,
        layer: u32,
        id: u32,
        gfx: *mut u8,
        name: *const u8,
    ) -> i32 {
        ((*(*this).vtable).load_sprite_by_layer)(this, layer, id, gfx, name)
    }

    /// Vtable slot 31: load sprite with flag.
    #[inline]
    pub unsafe fn load_sprite(
        this: *mut Self,
        layer: u32,
        id: u32,
        flag: u32,
        gfx: *mut u8,
        name: *const u8,
    ) -> i32 {
        ((*(*this).vtable).load_sprite)(this, layer, id, flag, gfx, name)
    }
}
