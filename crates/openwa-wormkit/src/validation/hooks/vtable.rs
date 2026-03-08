//! VTable hooking — swap function pointers in vtable memory.

use std::sync::atomic::{AtomicU32, Ordering};

use openwa_lib::rebase::rb;
use openwa_types::address::va;

use super::log_validation;

#[cfg(target_os = "windows")]
use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_READWRITE};

pub struct VtableHook {
    #[allow(dead_code)]
    slot_addr: *mut u32,
    original_fn: u32,
    #[allow(dead_code)]
    name: &'static str,
}

unsafe impl Send for VtableHook {}
unsafe impl Sync for VtableHook {}

impl VtableHook {
    #[cfg(target_os = "windows")]
    pub unsafe fn install(
        vtable_ghidra_addr: u32,
        slot_index: usize,
        hook_fn: u32,
        name: &'static str,
    ) -> Result<Self, String> {
        let slot_addr = rb(vtable_ghidra_addr + (slot_index as u32) * 4) as *mut u32;

        let mut old_protect: u32 = 0;
        let ok = VirtualProtect(slot_addr as *mut _, 4, PAGE_READWRITE, &mut old_protect);
        if ok == 0 {
            return Err(format!("VirtualProtect failed for {name} at 0x{:08X}", slot_addr as u32));
        }

        let original_fn = *slot_addr;
        *slot_addr = hook_fn;

        VirtualProtect(slot_addr as *mut _, 4, old_protect, &mut old_protect);

        let _ = log_validation(&format!(
            "  [VT] {name}: slot 0x{:08X}, orig 0x{original_fn:08X} -> hook 0x{hook_fn:08X}",
            slot_addr as u32
        ));

        Ok(VtableHook { slot_addr, original_fn, name })
    }

    pub fn original(&self) -> u32 {
        self.original_fn
    }
}

static ORIG_CTASK_PROCESS_FRAME: AtomicU32 = AtomicU32::new(0);

unsafe extern "fastcall" fn hook_ctask_process_frame(this: u32, _edx: u32, flags: u32) -> u32 {
    let orig: unsafe extern "fastcall" fn(u32, u32, u32) -> u32 =
        core::mem::transmute(ORIG_CTASK_PROCESS_FRAME.load(Ordering::Relaxed));
    orig(this, 0, flags)
}

static ORIG_CTASK_HANDLE_MESSAGE: AtomicU32 = AtomicU32::new(0);

unsafe extern "fastcall" fn hook_ctask_handle_message(
    this: u32, _edx: u32, sender: u32, msg_type: u32, size: u32, data: u32,
) -> u32 {
    let orig: unsafe extern "fastcall" fn(u32, u32, u32, u32, u32, u32) -> u32 =
        core::mem::transmute(ORIG_CTASK_HANDLE_MESSAGE.load(Ordering::Relaxed));
    orig(this, 0, sender, msg_type, size, data)
}

pub fn install() -> Result<(), String> {
    unsafe {
        let hook = VtableHook::install(
            va::CTASK_VTABLE, 7,
            hook_ctask_process_frame as *const () as u32,
            "CTask::ProcessFrame",
        )?;
        ORIG_CTASK_PROCESS_FRAME.store(hook.original(), Ordering::Relaxed);

        let hook = VtableHook::install(
            va::CTASK_VTABLE, 2,
            hook_ctask_handle_message as *const () as u32,
            "CTask::HandleMessage",
        )?;
        ORIG_CTASK_HANDLE_MESSAGE.store(hook.original(), Ordering::Relaxed);

        let _ = log_validation("  2 vtable hooks installed");
    }
    Ok(())
}
