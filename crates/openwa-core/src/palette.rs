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
    pub vtable: *mut u8,
    /// 0x004: Initialized to 0xFFFFFFFF during inline construction.
    pub _field_004: u32,
    /// 0x008-0x027: Unknown
    pub _unknown_008: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<Palette>() == 0x28);
