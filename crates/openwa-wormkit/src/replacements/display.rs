//! Display subsystem patches.
//!
//! Patches the DisplayBase primary vtable (0x6645F8) to replace _purecall
//! slots with safe Rust no-op stubs. This benefits both headless mode
//! (DisplayBase) and normal mode (DisplayGfx, which inherits from DisplayBase).

use core::mem::size_of;

use openwa_core::address::va;
use openwa_core::rebase::rb;
use crate::log_line;

/// The _purecall function address (calls abort).
const PURECALL: u32 = 0x005D_4E16;

/// Number of slots in the DisplayBase vtable.
const VTABLE_SLOTS: usize = 32;

unsafe extern "thiscall" fn noop_thiscall(_this: *mut u8) {}

pub fn install() -> Result<(), String> {
    let _ = log_line("[Display] Patching DisplayBase primary vtable");

    unsafe {
        let vtable_addr = rb(va::DISPLAY_BASE_VTABLE) as *mut u32;
        let purecall_addr = rb(PURECALL);
        let noop_addr = noop_thiscall as *const () as u32;

        // Make vtable writable.
        let mut old_protect: u32 = 0;
        let ok = windows_sys::Win32::System::Memory::VirtualProtect(
            vtable_addr as *mut core::ffi::c_void,
            (VTABLE_SLOTS * size_of::<u32>()) as usize,
            0x04, // PAGE_READWRITE
            &mut old_protect,
        );
        if ok == 0 {
            return Err("VirtualProtect failed on DisplayBase vtable".to_string());
        }

        let mut patched = 0u32;
        for i in 0..VTABLE_SLOTS {
            let slot = vtable_addr.add(i);
            if *slot == purecall_addr {
                *slot = noop_addr;
                patched += 1;
            }
        }

        // Restore protection.
        windows_sys::Win32::System::Memory::VirtualProtect(
            vtable_addr as *mut core::ffi::c_void,
            (VTABLE_SLOTS * size_of::<u32>()) as usize,
            old_protect,
            &mut old_protect,
        );

        let _ = log_line(&format!(
            "[Display]   Patched {patched}/{VTABLE_SLOTS} _purecall slots with no-op stubs"
        ));
    }

    Ok(())
}
