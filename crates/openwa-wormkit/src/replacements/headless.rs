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
    if flags & 0xF == 0x4 {
        6
    } else {
        1
    } // IDYES for MB_YESNO, IDOK otherwise
}

unsafe extern "system" fn hook_messagebox_w(
    _hwnd: u32,
    text: *const u16,
    caption: *const u16,
    flags: u32,
) -> i32 {
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
    if flags & 0xF == 0x4 {
        6
    } else {
        1
    }
}

pub fn install() -> Result<(), String> {
    if std::env::var("OPENWA_HEADLESS").is_err() {
        return Ok(());
    }

    let _ = log_line("[Headless] Suppressing all message boxes");

    unsafe {
        let module =
            windows_sys::Win32::System::LibraryLoader::GetModuleHandleA(b"user32.dll\0".as_ptr());
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
        ] {
            let proc =
                windows_sys::Win32::System::LibraryLoader::GetProcAddress(module, name.as_ptr());
            if let Some(addr) = proc {
                let target = addr as *mut core::ffi::c_void;
                if let Ok(trampoline) = minhook::MinHook::create_hook(target, hook_fn) {
                    let _ = minhook::MinHook::enable_hook(target);
                    let fn_name = std::str::from_utf8(&name[..name.len() - 1]).unwrap_or("?");
                    let _ = log_line(&format!(
                        "[Headless]   user32!{fn_name} hooked at 0x{:08X}, trampoline 0x{:08X}",
                        addr as usize, trampoline as usize
                    ));
                }
            }
        }
    }

    let _ = log_line("[Headless] All message box paths suppressed");
    Ok(())
}
