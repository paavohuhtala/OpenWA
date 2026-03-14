/// DisplayGfx — full display/graphics subsystem (derived from DDDisplayBase).
///
/// Constructor: DisplayGfx__Constructor (0x569C10), stdcall(this) → DisplayGfx*.
/// Size: 0x24E28 bytes.
///
/// Inheritance: DDDisplayBase (0x3560) → DisplayGfx (0x24E28).
/// The constructor calls DDDisplayBase__Constructor first, then sets the
/// DisplayGfx vtable (0x66A218) and initializes display-specific fields.
///
/// Created by GameEngine__InitHardware in normal (non-headless) mode.
/// Stored in the session's `display` field (shared with DDDisplayBase in headless).
///
/// OPAQUE: Internal layout not yet mapped.
#[repr(C)]
pub struct DisplayGfx {
    pub vtable: *mut u8,
    pub _unknown_004: [u8; 0x24E28 - 4],
}

const _: () = assert!(core::mem::size_of::<DisplayGfx>() == 0x24E28);

impl DisplayGfx {
    /// Allocate and construct a DisplayGfx via WA's native constructor.
    ///
    /// # Safety
    /// Must be called from within the WA.exe process.
    pub unsafe fn construct() -> *mut Self {
        use crate::address::va;
        use crate::rebase::rb;
        use crate::wa_alloc::WABox;
        let ctor: unsafe extern "stdcall" fn(*mut Self) -> *mut Self =
            core::mem::transmute(rb(va::DISPLAYGFX_CTOR) as usize);
        ctor(WABox::<Self>::alloc(0x24E28, 0x24E08).leak())
    }
}
