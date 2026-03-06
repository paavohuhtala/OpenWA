//! Win32 registry helpers for WA configuration.

use windows_sys::Win32::System::Registry::*;

const WA_REGISTRY_PATH: &str = "Software\\Team17SoftwareLTD\\WormsArmageddon\\";

/// Read an integer value from the WA registry.
///
/// Opens `HKCU\Software\Team17SoftwareLTD\WormsArmageddon\{section}\{key}`
/// and reads it as a DWORD. Returns `default` on any failure.
///
/// This replaces MFC's `CWinApp::GetProfileIntW` for the registry-mode path.
pub fn read_profile_int(section: &str, key: &str, default: u32) -> u32 {
    let mut full_path = String::with_capacity(WA_REGISTRY_PATH.len() + section.len() + 1);
    full_path.push_str(WA_REGISTRY_PATH);
    full_path.push_str(section);
    full_path.push('\0');

    let mut key_buf = String::with_capacity(key.len() + 1);
    key_buf.push_str(key);
    key_buf.push('\0');

    unsafe {
        let mut hkey: HKEY = core::ptr::null_mut();
        let status = RegOpenKeyExA(
            HKEY_CURRENT_USER,
            full_path.as_ptr(),
            0,
            KEY_READ,
            &mut hkey,
        );
        if status != 0 {
            return default;
        }

        let mut value: u32 = 0;
        let mut value_size: u32 = 4;
        let mut value_type: u32 = 0;
        let status = RegQueryValueExA(
            hkey,
            key_buf.as_ptr(),
            core::ptr::null_mut(),
            &mut value_type,
            &mut value as *mut u32 as *mut u8,
            &mut value_size,
        );
        RegCloseKey(hkey);

        if status == 0 && value_type == REG_DWORD {
            value
        } else {
            default
        }
    }
}

/// Read a string value from the WA registry.
///
/// Opens `HKCU\Software\Team17SoftwareLTD\WormsArmageddon\{section}\{key}`
/// and reads it as a REG_SZ string into `buf`. Returns the number of bytes
/// written (excluding null terminator), or 0 on failure.
pub fn read_profile_string(section: &str, key: &str, buf: &mut [u8]) -> usize {
    let mut full_path = String::with_capacity(WA_REGISTRY_PATH.len() + section.len() + 1);
    full_path.push_str(WA_REGISTRY_PATH);
    full_path.push_str(section);
    full_path.push('\0');

    let mut key_buf = String::with_capacity(key.len() + 1);
    key_buf.push_str(key);
    key_buf.push('\0');

    unsafe {
        let mut hkey: HKEY = core::ptr::null_mut();
        let status = RegOpenKeyExA(
            HKEY_CURRENT_USER,
            full_path.as_ptr(),
            0,
            KEY_READ,
            &mut hkey,
        );
        if status != 0 {
            return 0;
        }

        let mut value_size: u32 = buf.len() as u32;
        let mut value_type: u32 = 0;
        let status = RegQueryValueExA(
            hkey,
            key_buf.as_ptr(),
            core::ptr::null_mut(),
            &mut value_type,
            buf.as_mut_ptr(),
            &mut value_size,
        );
        RegCloseKey(hkey);

        if status == 0 && value_type == REG_SZ && value_size > 0 {
            // value_size includes null terminator
            (value_size - 1) as usize
        } else {
            0
        }
    }
}

/// Recursively delete a registry key and all its subkeys.
///
/// Replacement for WA's Registry__DeleteKeyRecursive (0x4E4D10).
/// Returns 0 on success, Win32 error code on failure.
pub unsafe fn delete_key_recursive(parent: HKEY, subkey: &str) -> u32 {
    if subkey.is_empty() {
        return 0x3F2; // ERROR_INVALID_PARAMETER equivalent used by original
    }

    let mut subkey_buf = String::with_capacity(subkey.len() + 1);
    subkey_buf.push_str(subkey);
    subkey_buf.push('\0');

    let mut hkey: HKEY = core::ptr::null_mut();
    let status = RegOpenKeyExA(
        parent,
        subkey_buf.as_ptr(),
        0,
        KEY_READ | KEY_ENUMERATE_SUB_KEYS,
        &mut hkey,
    );
    if status != 0 {
        return status;
    }

    // Enumerate and recursively delete all child keys
    let mut name_buf = vec![0u8; 256];
    loop {
        let mut name_len = name_buf.len() as u32;
        let status = RegEnumKeyExA(
            hkey,
            0, // always index 0 since we delete as we go
            name_buf.as_mut_ptr(),
            &mut name_len,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );

        if status == 0x103 {
            // ERROR_NO_MORE_ITEMS — all children deleted
            break;
        }
        if status != 0 {
            RegCloseKey(hkey);
            return status;
        }

        if name_len as usize == name_buf.len() {
            // Name buffer too small, grow it
            name_buf.resize(name_buf.len() * 2, 0);
            continue;
        }

        // Null-terminate and recurse
        name_buf[name_len as usize] = 0;
        let child_name = core::str::from_utf8_unchecked(&name_buf[..name_len as usize]);
        let result = delete_key_recursive(hkey, child_name);
        if result != 0 {
            RegCloseKey(hkey);
            return result;
        }
    }

    // All children deleted, now delete the key itself
    let status = RegDeleteKeyA(parent, subkey_buf.as_ptr());
    RegCloseKey(hkey);
    status
}
