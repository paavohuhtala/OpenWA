//! ASLR rebasing — convert Ghidra VAs to runtime addresses.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::address::va;

/// Delta to add to Ghidra addresses to get runtime addresses.
static REBASE_DELTA: AtomicU32 = AtomicU32::new(0);

/// Rebase a Ghidra VA to the actual runtime address.
#[inline]
pub fn rb(ghidra_addr: u32) -> u32 {
    ghidra_addr.wrapping_add(REBASE_DELTA.load(Ordering::Relaxed))
}

unsafe extern "system" {
    fn GetModuleHandleA(lpModuleName: *const u8) -> u32;
}

pub fn init() -> i32 {
    let base = unsafe { GetModuleHandleA(std::ptr::null()) };
    let delta = base.wrapping_sub(va::IMAGE_BASE);
    REBASE_DELTA.store(delta, Ordering::Relaxed);
    delta as i32
}
