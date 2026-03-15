//! Display subsystem patches.
//!
//! Patches DisplayBase vtables in WA.exe's .rdata:
//! - Primary vtable (0x6645F8): replaces _purecall slots with safe no-op stubs
//! - Headless vtable (0x66A0F8): replaces destructor with Rust version that
//!   correctly frees our Rust-allocated sprite cache sub-objects

use openwa_core::address::va;
use openwa_core::display::{DisplayBase, SpriteCacheWrapper, SpriteBufferCtrl};
use openwa_core::rebase::rb;
use openwa_core::vtable::patch_vtable;
use openwa_core::wa_alloc::wa_free;
use crate::log_line;

/// The _purecall function address (calls abort).
const PURECALL: u32 = 0x005D_4E16;

/// Number of slots in the DisplayBase vtable.
const VTABLE_SLOTS: usize = 32;

unsafe extern "thiscall" fn noop_thiscall(_this: *mut u8) {}

/// Rust destructor for headless DisplayBase. Frees the sprite cache chain
/// (wrapper → buffer_ctrl → buffer) that was allocated by new_headless().
unsafe extern "thiscall" fn headless_destructor(
    this: *mut DisplayBase,
    flags: u8,
) -> *mut DisplayBase {
    let wrapper_addr = (*this).sprite_cache;
    if wrapper_addr != 0 {
        let wrapper = wrapper_addr as *mut SpriteCacheWrapper;
        let ctrl_addr = (*wrapper).buffer_ctrl;
        if ctrl_addr != 0 {
            let ctrl = ctrl_addr as *mut SpriteBufferCtrl;
            let buf = (*ctrl).buffer;
            if buf != 0 {
                wa_free(buf as *mut u8);
            }
            wa_free(ctrl as *mut u8);
        }
        wa_free(wrapper as *mut u8);
    }
    if flags & 1 != 0 {
        wa_free(this as *mut u8);
    }
    this
}

pub fn install() -> Result<(), String> {
    let _ = log_line("[Display] Patching DisplayBase vtables");

    unsafe {
        let purecall_addr = rb(PURECALL);
        let noop_addr = noop_thiscall as *const () as u32;

        // Patch primary vtable (0x6645F8): replace _purecall with no-ops.
        let primary = rb(va::DISPLAY_BASE_VTABLE) as *mut u32;
        patch_vtable(primary, VTABLE_SLOTS, |vt| {
            let mut patched = 0u32;
            for i in 0..VTABLE_SLOTS {
                let slot = vt.add(i);
                if *slot == purecall_addr {
                    *slot = noop_addr;
                    patched += 1;
                }
            }
            let _ = log_line(&format!(
                "[Display]   Primary: patched {patched}/{VTABLE_SLOTS} _purecall → no-op"
            ));
        })?;

        // Patch headless vtable (0x66A0F8): replace destructor (slot 0)
        // with our Rust version that frees the Rust-allocated sprite cache.
        let headless = rb(va::DISPLAY_BASE_HEADLESS_VTABLE) as *mut u32;
        patch_vtable(headless, VTABLE_SLOTS, |vt| {
            *vt = headless_destructor as *const () as u32;
            let _ = log_line("[Display]   Headless: patched slot 0 (destructor) → Rust");
        })?;
    }

    Ok(())
}
