/// Palette — palette management object.
///
/// Vtable: 0x66A2E4.
/// Passed as "palette" parameter to DDGame__Constructor, stored at DDGame+0x010.
/// Size: 0x28 bytes.
///
/// PARTIAL: Only vtable and first init field known.
#[repr(C)]
pub struct Palette {
    /// 0x000: Vtable pointer (0x66A2E4)
    pub vtable: *const PaletteVtable,
    /// 0x004: Initialized to 0xFFFFFFFF during inline construction.
    pub _field_004: u32,
    /// 0x008-0x027: Unknown
    pub _unknown_008: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<Palette>() == 0x28);

/// Vtable for Palette (0x66A2E4).
///
/// Slots 2-4 are called by GameEngine__InitHardware after DDGameWrapper construction.
/// Slots 0-1 are unknown.
#[repr(C)]
pub struct PaletteVtable {
    /// [0]: Unknown
    pub _slot_0: usize,
    /// [1]: Unknown
    pub _slot_1: usize,
    /// [2]: set_mode(this, mode) — called with mode=7 during hardware init
    pub set_mode: unsafe extern "thiscall" fn(*mut Palette, u32),
    /// [3]: init(this) — called during hardware init
    pub init: unsafe extern "thiscall" fn(*mut Palette),
    /// [4]: reset(this) — called first during hardware init
    pub reset: unsafe extern "thiscall" fn(*mut Palette),
}

impl Palette {
    /// Create a new Palette with inline construction (no native C++ ctor).
    ///
    /// # Safety
    /// `vtable_addr` must be a valid rebased vtable pointer.
    pub unsafe fn new(vtable_addr: u32) -> Self {
        Self {
            vtable: vtable_addr as *const PaletteVtable,
            _field_004: 0xFFFF_FFFF,
            _unknown_008: [0; 0x20],
        }
    }

    /// Vtable[4]: Reset palette state.
    pub unsafe fn reset(&mut self) { vcall!(self, reset) }
    /// Vtable[3]: Initialize palette.
    pub unsafe fn init(&mut self) { vcall!(self, init) }
    /// Vtable[2]: Set palette mode.
    pub unsafe fn set_mode(&mut self, mode: u32) { vcall!(self, set_mode, mode) }
}
