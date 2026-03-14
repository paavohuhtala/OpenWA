/// GameStats — headless mode display stub.
///
/// Constructor: GameStats__Constructor (0x522DB0), stdcall(this) → GameStats*.
/// Vtable overlay: 0x66A0F8 (applied after constructor returns).
/// Size: 0x3560 bytes.
///
/// Created by GameEngine__InitHardware in headless mode (GameInfo+0xF914 != 0)
/// instead of DisplayGfx. Stored in the same GameSession+0xAC slot.
///
/// OPAQUE: Internal layout not yet mapped.
#[repr(C)]
pub struct GameStats {
    pub vtable: *mut u8,
    pub _unknown_004: [u8; 0x3560 - 4],
}

const _: () = assert!(core::mem::size_of::<GameStats>() == 0x3560);

impl GameStats {
    /// Allocate and construct a GameStats via WA's native constructor,
    /// then overlay the GameStats vtable (used in headless mode).
    ///
    /// # Safety
    /// Must be called from within the WA.exe process.
    pub unsafe fn construct() -> *mut Self {
        use crate::address::va;
        use crate::rebase::rb;
        use crate::wa_alloc::WABox;
        let stats = WABox::<Self>::alloc(0x3560, 0x3560).leak();
        let ctor: unsafe extern "stdcall" fn(*mut Self) -> *mut Self =
            core::mem::transmute(rb(va::GAMESTATS_CTOR) as usize);
        ctor(stats);
        (*stats).vtable = rb(va::GAMESTATS_VTABLE) as *mut u8;
        stats
    }
}
