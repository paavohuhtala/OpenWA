//! VTable hooking — swap function pointers in vtable memory.
//!
//! This is the simplest hooking approach: write a new function pointer
//! into a vtable slot and save the original. No trampoline, no prologue
//! analysis, no external dependencies.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::{log_line, rb};
use openwa_types::address::va;

#[cfg(target_os = "windows")]
use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_READWRITE};

/// A single vtable hook — remembers the slot address and original pointer.
pub struct VtableHook {
    #[allow(dead_code)]
    slot_addr: *mut u32,
    original_fn: u32,
    #[allow(dead_code)]
    name: &'static str,
}

// VtableHook contains a raw pointer but is only used from the main thread
unsafe impl Send for VtableHook {}
unsafe impl Sync for VtableHook {}

impl VtableHook {
    /// Install a vtable hook at `vtable_ghidra_addr + slot_index * 4`.
    ///
    /// # Safety
    /// - `vtable_ghidra_addr` must be a valid vtable base in .rdata
    /// - `hook_fn` must match the calling convention of the original method
    #[cfg(target_os = "windows")]
    pub unsafe fn install(
        vtable_ghidra_addr: u32,
        slot_index: usize,
        hook_fn: u32,
        name: &'static str,
    ) -> Result<Self, String> {
        let slot_addr = rb(vtable_ghidra_addr + (slot_index as u32) * 4) as *mut u32;

        // Make the vtable slot writable
        let mut old_protect: u32 = 0;
        let ok = VirtualProtect(slot_addr as *mut _, 4, PAGE_READWRITE, &mut old_protect);
        if ok == 0 {
            return Err(format!("VirtualProtect failed for {name} at 0x{:08X}", slot_addr as u32));
        }

        // Read original and write hook
        let original_fn = *slot_addr;
        *slot_addr = hook_fn;

        // Restore protection
        VirtualProtect(slot_addr as *mut _, 4, old_protect, &mut old_protect);

        let _ = log_line(&format!(
            "  [VT] {name}: slot 0x{:08X}, orig 0x{original_fn:08X} -> hook 0x{hook_fn:08X}",
            slot_addr as u32
        ));

        Ok(VtableHook { slot_addr, original_fn, name })
    }

    /// Get the original function pointer (for calling the original from the hook).
    pub fn original(&self) -> u32 {
        self.original_fn
    }
}

// ---------------------------------------------------------------------------
// Installed hooks — stored globally so originals can be called
// ---------------------------------------------------------------------------

/// Original CTask::ProcessFrame function pointer (thiscall: ECX = this)
static ORIG_CTASK_PROCESS_FRAME: AtomicU32 = AtomicU32::new(0);

/// Hook for CTask::ProcessFrame (vtable slot 7).
/// thiscall convention: ECX = this. We use fastcall (ECX = arg1, EDX = arg2).
unsafe extern "fastcall" fn hook_ctask_process_frame(this: u32, _edx: u32) -> u32 {
    // Call original
    let orig: unsafe extern "fastcall" fn(u32, u32) -> u32 =
        core::mem::transmute(ORIG_CTASK_PROCESS_FRAME.load(Ordering::Relaxed));
    orig(this, 0)
}

/// Original CTask::HandleMessage function pointer
static ORIG_CTASK_HANDLE_MESSAGE: AtomicU32 = AtomicU32::new(0);

/// Hook for CTask::HandleMessage (vtable slot 2).
/// thiscall(this, msg_id, param) — ECX = this, two stack params
unsafe extern "fastcall" fn hook_ctask_handle_message(this: u32, _edx: u32, msg_id: u32, param: u32) -> u32 {
    let orig: unsafe extern "fastcall" fn(u32, u32, u32, u32) -> u32 =
        core::mem::transmute(ORIG_CTASK_HANDLE_MESSAGE.load(Ordering::Relaxed));
    orig(this, 0, msg_id, param)
}

// ---------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------

pub fn install() -> Result<(), String> {
    unsafe {
        // CTask::ProcessFrame — slot 7
        let hook = VtableHook::install(
            va::CTASK_VTABLE,
            7,
            hook_ctask_process_frame as *const () as u32,
            "CTask::ProcessFrame",
        )?;
        ORIG_CTASK_PROCESS_FRAME.store(hook.original(), Ordering::Relaxed);

        // CTask::HandleMessage — slot 2
        let hook = VtableHook::install(
            va::CTASK_VTABLE,
            2,
            hook_ctask_handle_message as *const () as u32,
            "CTask::HandleMessage",
        )?;
        ORIG_CTASK_HANDLE_MESSAGE.store(hook.original(), Ordering::Relaxed);

        let _ = log_line("  2 vtable hooks installed");
    }

    Ok(())
}
