/// Music — music playback object.
///
/// Vtable: 0x66B3E0.
/// Constructor: FUN_0058BC10 (copies a string parameter).
/// Passed as "music" parameter to DDGame__Constructor, stored at DDGame+0x014.
///
/// OPAQUE: Size and fields not yet determined. Only vtable pointer defined.
#[repr(C)]
pub struct Music {
    /// 0x000: Vtable pointer (0x66B3E0)
    pub vtable: *mut u8,
}
