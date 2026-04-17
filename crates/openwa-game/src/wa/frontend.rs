//! WA-internal frontend function wrappers.

use crate::address::va;
use crate::rebase::rb;

/// Frontend__PaletteAnimation (0x422180)
///
/// `__usercall`: EAX = implicit param (from dialog+0x12c),
/// 2 stack params: &DAT_007be560 (palette data), palette_param (from dialog+0x134).
pub unsafe fn palette_animation(eax_value: u32, palette_param: u32) {
    let addr = rb(va::FRONTEND_PALETTE_ANIMATION);
    let data = rb(0x7be560);
    core::arch::asm!(
        "push {param}",
        "push {palette}",
        "call {func}",
        param = in(reg) palette_param,
        palette = in(reg) data,
        func = in(reg) addr,
        in("eax") eax_value,
        clobber_abi("C"),
    );
}
