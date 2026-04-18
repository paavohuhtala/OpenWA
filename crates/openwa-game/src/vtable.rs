//! Vtable patching utilities for WA.exe's .rdata vtables.

use core::mem::size_of;

/// Temporarily make a vtable region writable, run a patcher closure, then
/// restore the original memory protection.
///
/// # Safety
/// `vtable_addr` must point to a valid vtable with at least `slots` entries.
pub unsafe fn patch_vtable(
    vtable_addr: *mut u32,
    slots: usize,
    patcher: impl FnOnce(*mut u32),
) -> Result<(), &'static str> {
    unsafe {
        let mut old_protect: u32 = 0;
        let ok = windows_sys::Win32::System::Memory::VirtualProtect(
            vtable_addr as *mut core::ffi::c_void,
            slots * size_of::<u32>(),
            0x04, // PAGE_READWRITE
            &mut old_protect,
        );
        if ok == 0 {
            return Err("VirtualProtect failed on vtable");
        }

        patcher(vtable_addr);

        windows_sys::Win32::System::Memory::VirtualProtect(
            vtable_addr as *mut core::ffi::c_void,
            slots * size_of::<u32>(),
            old_protect,
            &mut old_protect,
        );
        Ok(())
    }
}
