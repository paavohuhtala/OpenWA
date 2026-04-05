/// OpenGLState — CPU-side OpenGL state object.
///
/// Created by OpenGLCPU__Constructor (0x5A0850).
/// Vtable: 0x6774C0
///
/// Small object (0x48 bytes) holding OpenGL rendering state.
/// The actual OpenGL context (HDC, HGLRC) is stored in DisplayGfx
/// fields during OpenGL__Init (0x59F000).
#[repr(C)]
pub struct OpenGLState {
    /// 0x00: Vtable pointer (0x6774C0)
    pub vtable: *mut u8,
    /// 0x04-0x0F: Unknown
    pub _unknown_04: [u8; 0x0C],
    /// 0x10: Width
    pub width: u32,
    /// 0x14: Unknown (init 0)
    pub _field_14: u32,
    /// 0x18-0x23: Unknown (init 0)
    pub _unknown_18: [u8; 0x0C],
    /// 0x24: Height
    pub height: u32,
    /// 0x28-0x3F: Unknown
    pub _unknown_28: [u8; 0x18],
    /// 0x40: Unknown (init 0)
    pub _field_40: u32,
    /// 0x44: Unknown (init 0)
    pub _field_44: u32,
}

const _: () = assert!(core::mem::size_of::<OpenGLState>() == 0x48);
