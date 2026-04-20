//! Headless mode support.
//!
//! When `OPENWA_HEADLESS=1` is set, hooks `user32!MessageBoxA` and
//! `MessageBoxW` via MinHook to auto-dismiss all message boxes. This allows
//! `/getlog` replay processing to run fully unattended.
//!
//! The hook targets the actual user32.dll function body because WA.exe
//! resolves MessageBoxA via `GetProcAddress` at runtime, bypassing its own
//! IAT. Only a direct MinHook on the function entry point catches this.

use crate::log_line;

unsafe extern "system" fn hook_messagebox_a(
    _hwnd: u32,
    text: *const u8,
    caption: *const u8,
    flags: u32,
) -> i32 {
    unsafe {
        let t = if text.is_null() {
            "<null>".to_string()
        } else {
            std::ffi::CStr::from_ptr(text as *const i8)
                .to_str()
                .unwrap_or("<invalid utf8>")
                .to_string()
        };
        let c = if caption.is_null() {
            "<null>".to_string()
        } else {
            std::ffi::CStr::from_ptr(caption as *const i8)
                .to_str()
                .unwrap_or("<invalid utf8>")
                .to_string()
        };
        let _ = log_line(&format!(
            "[Headless] Suppressed MessageBoxA: caption={c:?} text={t:?}"
        ));
        if flags & 0xF == 0x4 { 6 } else { 1 } // IDYES for MB_YESNO, IDOK otherwise
    }
}

unsafe extern "system" fn hook_messagebox_w(
    _hwnd: u32,
    text: *const u16,
    caption: *const u16,
    flags: u32,
) -> i32 {
    unsafe {
        let t = if text.is_null() {
            "<null>".to_string()
        } else {
            let len = (0..).take_while(|&i| *text.add(i) != 0).count();
            String::from_utf16_lossy(core::slice::from_raw_parts(text, len))
        };
        let c = if caption.is_null() {
            "<null>".to_string()
        } else {
            let len = (0..).take_while(|&i| *caption.add(i) != 0).count();
            String::from_utf16_lossy(core::slice::from_raw_parts(caption, len))
        };
        let _ = log_line(&format!(
            "[Headless] Suppressed MessageBoxW: caption={c:?} text={t:?}"
        ));
        if flags & 0xF == 0x4 { 6 } else { 1 }
    }
}

/// Suppress focus stealing — headless processes should never grab foreground.
unsafe extern "system" fn hook_set_foreground_window(_hwnd: u32) -> i32 {
    1 // pretend success
}

/// Suppress SetFocus — headless processes don't need keyboard focus.
unsafe extern "system" fn hook_set_focus(_hwnd: u32) -> u32 {
    0 // "previous focus window" = NULL
}

pub fn install() -> Result<(), String> {
    if std::env::var("OPENWA_HEADLESS").is_err() {
        return Ok(());
    }

    let _ = log_line("[Headless] Suppressing all message boxes");

    // Skip the "Loading, please wait..." splash dialog entirely.
    //
    // In Frontend__MainNavigationLoop at VA 0x004E9F19, a flag at
    // CWinApp+0x651 controls whether the loading dialog is created.
    // When zero (default), the code creates a dialog (resource 0x135D),
    // shows it, spawns a loading thread gated on WM_PAINT, and runs a
    // message pump. When non-zero, it calls ReplayLoader() directly —
    // no window, no thread, no focus steal.
    //
    // At VA 0x004E9F20 there's a `JNZ +0x1C7` (0F 85 C7 01 00 00) that
    // skips the dialog path. Patch it to an unconditional JMP so the
    // dialog is never created in headless mode.
    unsafe {
        use openwa_game::rebase::rb;
        let patch_addr = rb(0x004E9F20) as *mut u8;
        let mut old_protect: u32 = 0;
        let ok = windows_sys::Win32::System::Memory::VirtualProtect(
            patch_addr as *mut core::ffi::c_void,
            6,
            0x40, // PAGE_EXECUTE_READWRITE
            &mut old_protect,
        );
        if ok != 0 {
            assert_eq!(
                core::slice::from_raw_parts(patch_addr, 2),
                &[0x0F, 0x85],
                "expected JNZ (0F 85) at patch site"
            );
            // JNZ rel32 (6 bytes) → JMP rel32 (5 bytes) + NOP (1 byte)
            // Offset adjusts +1 because JMP is 1 byte shorter than JNZ.
            *patch_addr = 0xE9; // JMP rel32
            *patch_addr.add(1) = 0xC8;
            *patch_addr.add(2) = 0x01;
            *patch_addr.add(3) = 0x00;
            *patch_addr.add(4) = 0x00;
            *patch_addr.add(5) = 0x90; // NOP
            windows_sys::Win32::System::Memory::VirtualProtect(
                patch_addr as *mut core::ffi::c_void,
                6,
                old_protect,
                &mut old_protect,
            );
            let _ = log_line("[Headless] Patched loading dialog: JNZ → JMP (dialog skipped)");
        }
    }

    unsafe {
        let module = windows_sys::Win32::System::LibraryLoader::GetModuleHandleA(
            c"user32.dll".as_ptr().cast(),
        );
        if module.is_null() {
            return Err("user32.dll not loaded".to_string());
        }

        for (name, hook_fn) in [
            (
                &b"MessageBoxA\0"[..],
                hook_messagebox_a as *mut core::ffi::c_void,
            ),
            (
                &b"MessageBoxW\0"[..],
                hook_messagebox_w as *mut core::ffi::c_void,
            ),
            (
                &b"SetForegroundWindow\0"[..],
                hook_set_foreground_window as *mut core::ffi::c_void,
            ),
            (&b"SetFocus\0"[..], hook_set_focus as *mut core::ffi::c_void),
        ] {
            let proc =
                windows_sys::Win32::System::LibraryLoader::GetProcAddress(module, name.as_ptr());
            if let Some(addr) = proc {
                let target = addr as *mut core::ffi::c_void;
                if let Ok(trampoline) = minhook::MinHook::create_hook(target, hook_fn) {
                    let _ = minhook::MinHook::queue_enable_hook(target);
                    let fn_name = std::str::from_utf8(&name[..name.len() - 1]).unwrap_or("?");
                    let _ = log_line(&format!(
                        "[Headless]   user32!{fn_name} hooked at 0x{:08X}, trampoline 0x{:08X}",
                        addr as usize, trampoline as usize
                    ));
                }
            }
        }
    }

    let _ = log_line("[Headless] All message box and focus paths suppressed");

    // Hook CreateSemaphoreA to rename the "Worms Armageddon" instance
    // semaphore per-PID. Without this, concurrent WA.exe instances detect
    // each other and skip initialization or show warnings.
    unsafe {
        let k32 = windows_sys::Win32::System::LibraryLoader::GetModuleHandleA(
            c"kernel32.dll".as_ptr().cast(),
        );
        if !k32.is_null() {
            let proc = windows_sys::Win32::System::LibraryLoader::GetProcAddress(
                k32,
                c"CreateSemaphoreA".as_ptr().cast(),
            );
            if let Some(addr) = proc {
                let target = addr as *mut core::ffi::c_void;
                if let Ok(trampoline) = minhook::MinHook::create_hook(
                    target,
                    hook_create_semaphore_a as *mut core::ffi::c_void,
                ) {
                    let _ = minhook::MinHook::queue_enable_hook(target);
                    ORIG_CREATE_SEMAPHORE_A
                        .store(trampoline as u32, core::sync::atomic::Ordering::Relaxed);
                    let _ = log_line(&format!(
                        "[Headless] Hooked CreateSemaphoreA at 0x{:08X}",
                        addr as usize,
                    ));
                }
            }
        }
    }

    Ok(())
}

// ─── CreateSemaphoreA hook ──────────────────────────────────────────────────

use core::sync::atomic::{AtomicU32, Ordering};

static ORIG_CREATE_SEMAPHORE_A: AtomicU32 = AtomicU32::new(0);

type CreateSemaphoreAFn = unsafe extern "system" fn(
    lpSemaphoreAttributes: *mut core::ffi::c_void,
    lInitialCount: i32,
    lMaximumCount: i32,
    lpName: *const u8,
) -> *mut core::ffi::c_void;

unsafe extern "system" fn hook_create_semaphore_a(
    attrs: *mut core::ffi::c_void,
    initial: i32,
    max: i32,
    name: *const u8,
) -> *mut core::ffi::c_void {
    unsafe {
        let orig: CreateSemaphoreAFn =
            core::mem::transmute(ORIG_CREATE_SEMAPHORE_A.load(Ordering::Relaxed));

        if !name.is_null()
            && let Ok(s) = std::ffi::CStr::from_ptr(name as *const i8).to_str()
            && s == "Worms Armageddon"
        {
            let pid = std::process::id();
            let new_name = format!("Worms Armageddon_{pid}\0");
            let _ = log_line(&format!(
                "[Headless] Renamed semaphore \"{s}\" → \"Worms Armageddon_{pid}\""
            ));
            return orig(attrs, initial, max, new_name.as_ptr());
        }

        orig(attrs, initial, max, name)
    }
}
