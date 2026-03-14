/// DDDisplayBase — base class of the display subsystem hierarchy.
///
/// Constructor: DDDisplayBase__Constructor (0x522DB0), stdcall(this) → DDDisplayBase*.
/// Vtable (primary): 0x6645F8 (set by constructor).
/// Vtable (headless overlay): 0x66A0F8 (fills in stub slots for headless mode).
/// Size: 0x3560 bytes.
///
/// Inheritance:
/// ```text
/// DDDisplayBase (this)       ← vtable 0x6645F8 / 0x66A0F8
///   └─ DisplayGfx (derived)  ← vtable 0x66A218
/// ```
///
/// In headless mode (`GameInfo.headless_mode != 0`), only the base is constructed
/// with the headless vtable overlay. In normal mode, `DisplayGfx` (derived) is
/// constructed instead. The session's `display` field holds a polymorphic pointer
/// to whichever variant.
#[repr(C)]
pub struct DDDisplayBase {
    pub vtable: *mut u8,
    pub _unknown_004: [u8; 0x3560 - 4],
}

const _: () = assert!(core::mem::size_of::<DDDisplayBase>() == 0x3560);

impl DDDisplayBase {
    /// Allocate and construct a DDDisplayBase for headless mode.
    ///
    /// Calls WA's native constructor, then overlays the headless vtable.
    ///
    /// # Safety
    /// Must be called from within the WA.exe process.
    pub unsafe fn construct() -> *mut Self {
        use crate::address::va;
        use crate::rebase::rb;
        use crate::wa_alloc::WABox;
        let this = WABox::<Self>::alloc(0x3560, 0x3560).leak();
        let ctor: unsafe extern "stdcall" fn(*mut Self) -> *mut Self =
            core::mem::transmute(rb(va::DD_DISPLAY_BASE_CTOR) as usize);
        ctor(this);
        (*this).vtable = rb(va::DD_DISPLAY_BASE_HEADLESS_VTABLE) as *mut u8;
        this
    }
}
