//! Bridge for `sprintf` into one of 8 rotating 16-KiB scratch buffers
//! (WA address `0x005978A0`). cdecl varargs; the callee picks the next
//! buffer slot, writes the formatted output, and returns a pointer into
//! it. WA callers don't free the buffer — it gets overwritten on the next
//! cycle through the 8-slot ring.

use core::ffi::c_char;

use crate::address::va;
use crate::rebase::rb;

static mut SPRINTF_ADDR: u32 = 0;

/// Initialize the bridge address. Called from
/// `dispatch_frame::init_dispatch_addrs` at DLL load.
pub unsafe fn init_addrs() {
    unsafe {
        SPRINTF_ADDR = rb(va::SPRINTF_ROTATING_BUFFER);
    }
}

/// Three-argument variant — the only shape WA's render-tail-func and
/// ESC-menu callers use. WA pushes 3 varargs even when the format string
/// only consumes one (e.g. "First Team to %d Wins" reads only `a3`).
pub unsafe fn sprintf_3(format: *const c_char, a1: u32, a2: u32, a3: u32) -> *const c_char {
    unsafe {
        let func: unsafe extern "cdecl" fn(*const c_char, u32, u32, u32) -> *const c_char =
            core::mem::transmute(SPRINTF_ADDR as usize);
        func(format, a1, a2, a3)
    }
}
