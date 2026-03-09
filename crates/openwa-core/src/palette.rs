/// Palette — palette management object.
///
/// Vtable: 0x66A2E4.
/// Passed as "palette" parameter to DDGame__Constructor, stored at DDGame+0x010.
///
/// OPAQUE: Size and fields not yet determined. Only vtable pointer defined.
#[repr(C)]
pub struct Palette {
    /// 0x000: Vtable pointer (0x66A2E4)
    pub vtable: *mut u8,
}
