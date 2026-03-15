/// DDDisplay — display/rendering subsystem.
///
/// Constructor: DDDisplay__Init (0x569D00).
/// Vtable: 0x66A218.
/// Destructor: FUN_00569CE0.
///
/// Manages display mode, dimensions, palette, and HWND.
/// Contains DrawTextOnBitmap (thiscall) and ConstructTextbox methods.
/// The OpenGL context (HDC, HGLRC) is also stored here when GL mode is active.
///
/// OPAQUE: Size not yet determined. Only vtable pointer defined.
#[repr(C)]
pub struct DDDisplay {
    /// 0x000: Vtable pointer (0x66A218)
    pub vtable: *mut u8,
}
