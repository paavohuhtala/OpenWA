/// Vtable for Palette (0x66A2E4).
///
/// Slots 2-4 are called by GameEngine__InitHardware after GameRuntime construction.
/// Slots 0-1 are unknown.
#[openwa_game::vtable(size = 5, va = 0x0066A2E4, class = "Palette")]
pub struct PaletteVtable {
    /// set_mode(this, mode) — called with mode=7 during hardware init
    #[slot(2)]
    pub set_mode: fn(this: *mut Palette, mode: u32),
    /// init — called during hardware init
    #[slot(3)]
    pub init: fn(this: *mut Palette),
    /// reset — called first during hardware init
    #[slot(4)]
    pub reset: fn(this: *mut Palette),
}

/// Palette — palette management object.
///
/// Vtable: 0x66A2E4.
/// Passed as "palette" parameter to GameWorld__Constructor, stored at GameWorld+0x010.
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

// Generate calling wrappers: Palette::set_mode(), Palette::init(), Palette::reset()
bind_PaletteVtable!(Palette, vtable);

impl Palette {
    /// Create a new Palette with inline construction (no native C++ ctor).
    ///
    /// # Safety
    /// `vtable_addr` must be a valid rebased vtable pointer.
    pub unsafe fn new(vtable_addr: u32) -> Self {
        Self {
            vtable: vtable_addr as *const PaletteVtable,
            _field_004: 0xFFFFFFFF,
            _unknown_008: [0; 0x20],
        }
    }
}
