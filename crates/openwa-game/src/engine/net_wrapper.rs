use crate::wa_alloc::wa_malloc_struct_zeroed;

/// DDNetGameWrapper — network game wrapper.
///
/// Constructor: DDNetGameWrapper__Constructor (0x56D1F0), stdcall(this) → DDNetGameWrapper*.
/// Size: 0x2C bytes.
///
/// Created by GameEngine__InitHardware (always, after DDGameWrapper).
/// Stored at GameSession+0xC0.
///
/// OPAQUE: Internal layout not yet mapped.
#[repr(C)]
pub struct DDNetGameWrapper {
    pub _data: [u8; 0x2C],
}

const _: () = assert!(core::mem::size_of::<DDNetGameWrapper>() == 0x2C);

impl DDNetGameWrapper {
    /// Allocate and construct a DDNetGameWrapper via WA's native constructor.
    ///
    /// # Safety
    /// Must be called from within the WA.exe process.
    pub unsafe fn construct() -> *mut Self {
        unsafe {
            use crate::address::va;
            use crate::rebase::rb;
            let ctor: unsafe extern "stdcall" fn(*mut Self) -> *mut Self =
                core::mem::transmute(rb(va::DDNETGAME_WRAPPER_CTOR) as usize);
            ctor(wa_malloc_struct_zeroed::<Self>())
        }
    }
}
