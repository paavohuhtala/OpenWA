# Configuration System Port — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Port 7 WA.exe configuration functions (theme I/O, registry management, game options loading) to Rust.

**Architecture:** New `config.rs` replacement module in openwa-dll hooks 7 functions. Registry helpers live in openwa-lib for reuse. Uses `windows` crate for Win32 registry/MessageBox APIs, `std::fs` for theme file I/O.

**Tech Stack:** Rust, `windows` crate (Win32 registry + UI), MinHook inline hooks, `std::fs`

---

### Task 1: Add `windows` crate dependency to openwa-lib

**Files:**
- Modify: `crates/openwa-lib/Cargo.toml`

**Step 1: Add the dependency**

```toml
[dependencies]
openwa-types = { path = "../openwa-types" }

[target.'cfg(target_os = "windows")'.dependencies]
windows-sys = { version = "0.59", features = [
    "Win32_System_Registry",
    "Win32_UI_WindowsAndMessaging",
] }
```

Note: Use `windows-sys` (not `windows`) — it's the raw FFI bindings crate, already used by openwa-dll. Lower overhead, no COM wrappers.

**Step 2: Build to verify**

Run: `cargo build --release -p openwa-lib`
Expected: clean build

**Step 3: Commit**

```
feat: add windows-sys registry + UI features to openwa-lib
```

---

### Task 2: Add address constants

**Files:**
- Modify: `crates/openwa-types/src/address.rs`

**Step 1: Add constants**

Add to the `va` module, in a new `// === Configuration / registry ===` section after the scheme section:

```rust
// === Configuration / registry ===

/// Theme file size check: cdecl() -> u32 (file length or 0)
pub const THEME_GET_FILE_SIZE: u32 = 0x0044_BA80;
/// Theme file load: stdcall(dest_buffer)
pub const THEME_LOAD: u32 = 0x0044_BB20;
/// Theme file save: stdcall(buffer, size)
pub const THEME_SAVE: u32 = 0x0044_BBC0;
/// Recursive registry key deletion: stdcall(hkey, subkey) -> i32
pub const REGISTRY_DELETE_KEY_RECURSIVE: u32 = 0x004E_4D10;
/// Registry cleanup — deletes all 4 subsections + clears INI: stdcall(struct_ptr)
pub const REGISTRY_CLEAN_ALL: u32 = 0x004C_90D0;
/// Loads game options from registry into GameInfo struct: stdcall(game_info_ptr)
pub const GAMEINFO_LOAD_OPTIONS: u32 = 0x0046_0AC0;
/// Reads CrashReportURL from Options registry key: cdecl() -> *const u8
pub const OPTIONS_GET_CRASH_REPORT_URL: u32 = 0x005A_63F0;
```

**Step 2: Build to verify**

Run: `cargo build --release -p openwa-types`
Expected: clean build

**Step 3: Commit**

```
feat: add configuration system address constants
```

---

### Task 3: Add registry helpers to openwa-lib

**Files:**
- Create: `crates/openwa-lib/src/wa/registry.rs`
- Modify: `crates/openwa-lib/src/wa/mod.rs`

**Step 1: Add `pub mod registry;` to mod.rs**

After the existing `pub mod resource;` line:

```rust
pub mod registry;
```

**Step 2: Create registry.rs**

```rust
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
        let mut hkey: HKEY = 0;
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
        let mut hkey: HKEY = 0;
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
pub unsafe fn delete_key_recursive(parent: HKEY, subkey: &str) -> i32 {
    if subkey.is_empty() {
        return 0x3F2; // ERROR_INVALID_PARAMETER equivalent used by original
    }

    let mut subkey_buf = String::with_capacity(subkey.len() + 1);
    subkey_buf.push_str(subkey);
    subkey_buf.push('\0');

    let mut hkey: HKEY = 0;
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
```

**Step 3: Build to verify**

Run: `cargo build --release -p openwa-lib`
Expected: clean build

**Step 4: Commit**

```
feat: add Win32 registry helpers to openwa-lib
```

---

### Task 4: Create config.rs with Theme hooks

**Files:**
- Create: `crates/openwa-dll/src/replacements/config.rs`
- Modify: `crates/openwa-dll/src/replacements/mod.rs`

**Step 1: Add `mod config;` to mod.rs and call install**

```rust
mod config;
mod frontend;
mod scheme;

pub fn install_all() -> Result<(), String> {
    frontend::install()?;
    scheme::install()?;
    config::install()?;
    Ok(())
}
```

**Step 2: Create config.rs with theme functions**

```rust
//! Configuration system hooks.
//!
//! Replaces WA.exe configuration functions with Rust implementations:
//! - Theme__GetFileSize (0x44BA80): theme file size query
//! - Theme__Load (0x44BB20): theme file read
//! - Theme__Save (0x44BBC0): theme file write
//! - Registry__DeleteKeyRecursive (0x4E4D10): recursive registry deletion
//! - Registry__CleanAll (0x4C90D0): full registry cleanup
//! - GameInfo__LoadOptions (0x460AC0): game options from registry
//! - Options__GetCrashReportURL (0x5A63F0): crash report URL from registry

use crate::log_line;
use openwa_lib::rebase::rb;
use openwa_types::address::va;

const THEME_PATH: &str = "data\\current.thm";

// ============================================================
// Theme__GetFileSize replacement (0x44BA80)
// ============================================================

/// Rust replacement for Theme__GetFileSize.
/// cdecl() -> u32 (file length, or 0 if missing)
unsafe extern "cdecl" fn hook_theme_get_file_size() -> u32 {
    match std::fs::metadata(THEME_PATH) {
        Ok(m) => m.len() as u32,
        Err(_) => 0,
    }
}

// ============================================================
// Theme__Load replacement (0x44BB20)
// ============================================================

/// Rust replacement for Theme__Load.
/// stdcall(dest_buffer: *mut u8)
unsafe extern "stdcall" fn hook_theme_load(dest: u32) {
    match std::fs::read(THEME_PATH) {
        Ok(data) => {
            core::ptr::copy_nonoverlapping(data.as_ptr(), dest as *mut u8, data.len());
        }
        Err(_) => {
            show_error_message("ERROR: NO CURRENT.THM FILE FOUND");
        }
    }
}

// ============================================================
// Theme__Save replacement (0x44BBC0)
// ============================================================

/// Rust replacement for Theme__Save.
/// stdcall(buffer: *const u8, size: u32)
unsafe extern "stdcall" fn hook_theme_save(buffer: u32, size: u32) {
    let data = core::slice::from_raw_parts(buffer as *const u8, size as usize);
    if let Err(_) = std::fs::write(THEME_PATH, data) {
        show_error_message("ERROR: Could Not create CURRENT.THM File");
    }
}

/// Show an error message box, matching AfxMessageBox behavior.
fn show_error_message(msg: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    let mut msg_buf: Vec<u8> = msg.bytes().collect();
    msg_buf.push(0);
    unsafe {
        MessageBoxA(0, msg_buf.as_ptr(), core::ptr::null(), MB_OK);
    }
}

// ============================================================
// Hook installation
// ============================================================

pub fn install() -> Result<(), String> {
    unsafe {
        let _ = crate::hook::install(
            "Theme__GetFileSize",
            va::THEME_GET_FILE_SIZE,
            hook_theme_get_file_size as *const (),
        )?;

        let _ = crate::hook::install(
            "Theme__Load",
            va::THEME_LOAD,
            hook_theme_load as *const (),
        )?;

        let _ = crate::hook::install(
            "Theme__Save",
            va::THEME_SAVE,
            hook_theme_save as *const (),
        )?;
    }

    Ok(())
}
```

**Step 3: Add windows-sys dependency to openwa-dll Cargo.toml**

The `windows-sys` dep in openwa-dll needs the UI feature too. Update the existing entry:

```toml
[target.'cfg(target_os = "windows")'.dependencies]
windows-sys = { version = "0.59", features = [
    "Win32_System_Memory",
    "Win32_UI_WindowsAndMessaging",
] }
```

**Step 4: Build to verify**

Run: `cargo build --release -p openwa-dll`
Expected: clean build

**Step 5: Deploy + test theme**

Copy DLL, launch game. Theme should load normally (visual appearance unchanged). Check OpenWA.log for hook messages.

**Step 6: Commit**

```
feat: add Theme file I/O hooks (GetFileSize, Load, Save)
```

---

### Task 5: Add Registry hooks

**Files:**
- Modify: `crates/openwa-dll/src/replacements/config.rs`

**Step 1: Add registry hook functions**

Add after the Theme section, before `install()`:

```rust
// ============================================================
// Registry__DeleteKeyRecursive replacement (0x4E4D10)
// ============================================================

/// Rust replacement for Registry__DeleteKeyRecursive.
/// stdcall(hkey: HKEY, subkey: *const u8) -> i32
unsafe extern "stdcall" fn hook_delete_key_recursive(hkey: u32, subkey: u32) -> i32 {
    use windows_sys::Win32::System::Registry::HKEY;

    let c_subkey = std::ffi::CStr::from_ptr(subkey as *const i8);
    let subkey_str = c_subkey.to_string_lossy();

    let _ = log_line(&format!("[Config] DeleteKeyRecursive: {subkey_str}"));

    let result = openwa_lib::wa::registry::delete_key_recursive(
        hkey as HKEY,
        &subkey_str,
    );

    let _ = log_line(&format!("[Config] DeleteKeyRecursive result: {result}"));
    result
}

// ============================================================
// Registry__CleanAll replacement (0x4C90D0)
// ============================================================

/// Rust replacement for Registry__CleanAll.
/// stdcall(struct_ptr: u32)
unsafe extern "stdcall" fn hook_registry_clean_all(struct_ptr: u32) {
    use windows_sys::Win32::System::Registry::HKEY_CURRENT_USER;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    let _ = log_line("[Config] CleanAll: deleting registry sections");

    let sections = [
        "Software\\Team17SoftwareLTD\\WormsArmageddon\\Data",
        "Software\\Team17SoftwareLTD\\WormsArmageddon\\Options",
        "Software\\Team17SoftwareLTD\\WormsArmageddon\\ExportVideo",
        "Software\\Team17SoftwareLTD\\WormsArmageddon\\VSyncAssist",
    ];

    for section in &sections {
        openwa_lib::wa::registry::delete_key_recursive(HKEY_CURRENT_USER, section);
    }

    // Clear the NetSettings INI section
    WriteProfileSectionA(b"NetSettings\0".as_ptr(), b"\0".as_ptr());

    // Set struct_ptr + 0xE0 = 0
    *((struct_ptr + 0xE0) as *mut u8) = 0;

    let _ = log_line("[Config] CleanAll completed");
}
```

**Step 2: Wire up in install()**

Add before the closing `Ok(())`:

```rust
        let _ = crate::hook::install(
            "Registry__DeleteKeyRecursive",
            va::REGISTRY_DELETE_KEY_RECURSIVE,
            hook_delete_key_recursive as *const (),
        )?;

        let _ = crate::hook::install(
            "Registry__CleanAll",
            va::REGISTRY_CLEAN_ALL,
            hook_registry_clean_all as *const (),
        )?;
```

**Step 3: Build to verify**

Run: `cargo build --release -p openwa-dll`
Expected: clean build

**Step 4: Commit**

```
feat: add Registry cleanup hooks (DeleteKeyRecursive, CleanAll)
```

---

### Task 6: Add GameInfo__LoadOptions hook

**Files:**
- Modify: `crates/openwa-dll/src/replacements/config.rs`
- Modify: `crates/openwa-types/src/address.rs` (global data addresses)

**Step 1: Add global data address constants**

Add to `address.rs` in the `// === Global variables (in .data) ===` section:

```rust
/// Base directory string (null-terminated)
pub const G_BASE_DIR: u32 = 0x0088_E282;
/// 64-byte data block copied into GameInfo+0xF485
pub const G_GAMEINFO_BLOCK_F485: u32 = 0x0088_DFF3;
/// "data\land.dat" string constant (14 bytes)
pub const G_LAND_DAT_STRING: u32 = 0x0064_DA58;
/// Unknown byte read into GameInfo+0xF3A0
pub const G_CONFIG_BYTE_F3A0: u32 = 0x007C_0D38;
/// 5 DWORDs: GameInfo offsets +0xF3B4..+0xF3D0
pub const G_CONFIG_DWORDS_F3B4: u32 = 0x0088_E39C;
/// Guard flag for conditional config copies
pub const G_CONFIG_GUARD: u32 = 0x0088_C374;
/// 4 DWORDs (conditional): GameInfo offsets +0xF3F4..+0xF400
pub const G_CONFIG_DWORDS_F3F4: u32 = 0x0088_E3B8;
/// DWORD → GameInfo+0xDAE8
pub const G_CONFIG_DWORD_DAE8: u32 = 0x0088_E390;
/// 2 DWORDs → GameInfo+0xF3D4, +0xF3D8
pub const G_CONFIG_DWORDS_F3D4: u32 = 0x0088_E3B0;
/// 3 DWORDs → GameInfo+0xF3C4..+0xF3CC
pub const G_CONFIG_DWORDS_F3C4: u32 = 0x0088_E400;
/// DWORD → GameInfo+0xF3E4
pub const G_CONFIG_DWORD_F3E4: u32 = 0x0088_E44C;
/// Streams directory path buffer
pub const G_STREAMS_DIR: u32 = 0x0088_AE18;
/// Random stream indices (16 entries)
pub const G_STREAM_INDICES: u32 = 0x0088_AE9C;
/// Stream index end sentinel
pub const G_STREAM_INDICES_END: u32 = 0x0088_AEDC;
/// DAT_0088E394 flag for stream volume
pub const G_STREAM_FLAG: u32 = 0x0088_E394;
/// Stream volume byte
pub const G_STREAM_VOLUME: u32 = 0x0088_AEDD;
```

**Step 2: Add LoadOptions hook function**

Add to config.rs before `install()`:

```rust
// ============================================================
// GameInfo__LoadOptions replacement (0x460AC0)
// ============================================================

/// Rust replacement for GameInfo__LoadOptions.
/// stdcall(game_info: u32)
///
/// Reads game options from the Windows registry and copies various globals
/// into the GameInfo struct at known offsets.
unsafe extern "stdcall" fn hook_load_options(gi: u32) {
    use openwa_lib::wa::registry::read_profile_int;

    let _ = log_line("[Config] LoadOptions: loading game options from registry");

    // Format speech path: "%s\user\speech"
    let base_dir = rb(va::G_BASE_DIR) as *const u8;
    let speech_dest = (gi + 0xF404) as *mut u8;
    let base_str = std::ffi::CStr::from_ptr(base_dir as *const i8);
    let speech_path = format!("{}\\user\\speech\0", base_str.to_string_lossy());
    core::ptr::copy_nonoverlapping(
        speech_path.as_ptr(),
        speech_dest,
        speech_path.len(),
    );

    // Copy 64 bytes from global 0x88DFF3 → GameInfo+0xF485
    core::ptr::copy_nonoverlapping(
        rb(va::G_GAMEINFO_BLOCK_F485) as *const u8,
        (gi + 0xF485) as *mut u8,
        64,
    );

    // Format streams directory and randomize stream indices
    let streams_dest = rb(va::G_STREAMS_DIR) as *mut u8;
    let streams_path = format!("{}\\streams\0", base_str.to_string_lossy());
    core::ptr::copy_nonoverlapping(
        streams_path.as_ptr(),
        streams_dest,
        streams_path.len(),
    );

    // Randomize stream indices (16 entries, each rand() % 11 + 1)
    let indices = rb(va::G_STREAM_INDICES) as *mut u32;
    let indices_end = rb(va::G_STREAM_INDICES_END) as usize;
    let mut ptr = indices;
    while (ptr as usize) < indices_end {
        extern "cdecl" { fn rand() -> i32; }
        *ptr = (rand() % 11 + 1) as u32;
        ptr = ptr.add(1);
    }

    // Stream volume: 0x10 if flag set, else 0
    let stream_vol_addr = rb(va::G_STREAM_INDICES_END) as *mut u8;
    *stream_vol_addr = if *(rb(va::G_STREAM_FLAG) as *const u32) != 0 { 0x10 } else { 0 };
    // Secondary volume byte
    *(rb(va::G_STREAM_VOLUME) as *mut u8) = 0x4B;

    // Copy "data\land.dat" string (14 bytes) → GameInfo+0xDAEC
    core::ptr::copy_nonoverlapping(
        rb(va::G_LAND_DAT_STRING) as *const u8,
        (gi + 0xDAEC) as *mut u8,
        14,
    );

    // Copy byte from global → GameInfo+0xF3A0
    *((gi + 0xF3A0) as *mut u8) = *(rb(va::G_CONFIG_BYTE_F3A0) as *const u8);

    // Read registry values from "Options" section
    let detail = read_profile_int("Options", "DetailLevel", 5);
    *((gi + 0xF3A1) as *mut u8) = detail as u8;

    // Zero 2 bytes at +0xF3F0
    *((gi + 0xF3F0) as *mut u16) = 0;

    // Copy 5 DWORDs from globals → GameInfo+0xF3B4..+0xF3D0
    let src = rb(va::G_CONFIG_DWORDS_F3B4) as *const u32;
    for i in 0u32..5 {
        let offset = 0xF3B4 + i * 4;
        *((gi + offset) as *mut u32) = *src.add(i as usize);
    }

    // Conditional copy: 4 DWORDs if guard == 0
    if *(rb(va::G_CONFIG_GUARD) as *const u32) == 0 {
        let src = rb(va::G_CONFIG_DWORDS_F3F4) as *const u32;
        for i in 0u32..4 {
            let offset = 0xF3F4 + i * 4;
            *((gi + offset) as *mut u32) = *src.add(i as usize);
        }
    }

    // Single DWORDs from globals
    *((gi + 0xDAE8) as *mut u32) = *(rb(va::G_CONFIG_DWORD_DAE8) as *const u32);

    let src_d4 = rb(va::G_CONFIG_DWORDS_F3D4) as *const u32;
    *((gi + 0xF3D4) as *mut u32) = *src_d4;
    *((gi + 0xF3D8) as *mut u32) = *src_d4.add(1);

    // EnergyBar
    let energy = read_profile_int("Options", "EnergyBar", 1);
    *((gi + 0xF3A2) as *mut u8) = energy as u8;

    // 3 DWORDs from globals → +0xF3C4..+0xF3CC
    let src_c4 = rb(va::G_CONFIG_DWORDS_F3C4) as *const u32;
    for i in 0u32..3 {
        let offset = 0xF3C4 + i * 4;
        *((gi + offset) as *mut u32) = *src_c4.add(i as usize);
    }

    // Remaining registry values
    let info_trans = read_profile_int("Options", "InfoTransparency", 0);
    *((gi + 0xF3A3) as *mut u8) = info_trans as u8;

    let info_spy = read_profile_int("Options", "InfoSpy", 1);
    *((gi + 0xF3A4) as *mut u8) = if info_spy != 0 { 1 } else { 0 };

    let chat_pinned = read_profile_int("Options", "ChatPinned", 0);
    *((gi + 0xF3A5) as *mut u8) = chat_pinned as u8;

    let chat_lines = read_profile_int("Options", "ChatLines", 0);
    *((gi + 0xF3A8) as *mut u32) = chat_lines;

    let pinned_lines = read_profile_int("Options", "PinnedChatLines", 0xFFFFFFFF);
    *((gi + 0xF3AC) as *mut u32) = pinned_lines;

    let home_lock = read_profile_int("Options", "HomeLock", 0);
    *((gi + 0xF3B0) as *mut u8) = home_lock as u8;

    // BackgroundDebrisParallax: clamp to i16 range, then << 16
    let mut parallax = read_profile_int("Options", "BackgroundDebrisParallax", 0x50);
    let parallax_i32 = parallax as i32;
    if parallax_i32 < -0x8000 || parallax_i32 > 0x7FFF {
        // Clamp to i16 range
        if parallax_i32 < 0 {
            parallax = (-0x8000i32) as u32;
        } else {
            parallax = 0x7FFF;
        }
    }
    *((gi + 0xF3E8) as *mut u32) = parallax << 16;

    let onomatopoeia = read_profile_int("Options", "TopmostExplosionOnomatopoeia", 0);
    *((gi + 0xF3EC) as *mut u32) = onomatopoeia;

    let capture_png = read_profile_int("Options", "CaptureTransparentPNGs", 0);
    *((gi + 0xF3DC) as *mut u32) = capture_png;

    // CameraUnlockMouseSpeed: clamp to max 0xB504, then square
    let mut mouse_speed = read_profile_int("Options", "CameraUnlockMouseSpeed", 0x10);
    if mouse_speed > 0xB504 {
        if (mouse_speed as i32) < 0 {
            mouse_speed = 0; // negative → 0
        } else {
            mouse_speed = 0xB504;
        }
    }
    *((gi + 0xF3E0) as *mut u32) = mouse_speed * mouse_speed;

    // Final global DWORD
    *((gi + 0xF3E4) as *mut u32) = *(rb(va::G_CONFIG_DWORD_F3E4) as *const u32);

    let _ = log_line("[Config] LoadOptions completed (Rust)");
}
```

**Step 3: Wire up in install()**

Add to the unsafe block in `install()`:

```rust
        let _ = crate::hook::install(
            "GameInfo__LoadOptions",
            va::GAMEINFO_LOAD_OPTIONS,
            hook_load_options as *const (),
        )?;
```

**Step 4: Build to verify**

Run: `cargo build --release -p openwa-dll`
Expected: clean build

**Step 5: Deploy + test options**

Launch game, change Detail Level in options, restart, verify it persists. Check OpenWA.log.

**Step 6: Commit**

```
feat: add GameInfo__LoadOptions hook (registry reads)
```

---

### Task 7: Add Options__GetCrashReportURL hook

**Files:**
- Modify: `crates/openwa-dll/src/replacements/config.rs`
- Modify: `crates/openwa-types/src/address.rs`

**Step 1: Add global address for the URL buffer**

Add to address.rs globals section:

```rust
/// CrashReportURL static buffer (0x400 bytes)
pub const G_CRASH_REPORT_URL: u32 = 0x0079_FFD8;
```

**Step 2: Add hook function**

Add to config.rs before `install()`:

```rust
// ============================================================
// Options__GetCrashReportURL replacement (0x5A63F0)
// ============================================================

/// Rust replacement for Options__GetCrashReportURL.
/// cdecl() -> *const u8 (pointer to static buffer, or null)
unsafe extern "cdecl" fn hook_get_crash_report_url() -> u32 {
    let buf = rb(va::G_CRASH_REPORT_URL) as *mut u8;
    let mut buf_slice = core::slice::from_raw_parts_mut(buf, 0x401);

    let len = openwa_lib::wa::registry::read_profile_string(
        "Options",
        "CrashReportURL",
        &mut buf_slice[..0x400],
    );

    if len > 0 {
        // Null-terminate
        *buf.add(len) = 0;
        buf as u32
    } else {
        0 // null pointer = not found
    }
}
```

**Step 3: Wire up in install()**

```rust
        let _ = crate::hook::install(
            "Options__GetCrashReportURL",
            va::OPTIONS_GET_CRASH_REPORT_URL,
            hook_get_crash_report_url as *const (),
        )?;
```

**Step 4: Build to verify**

Run: `cargo build --release -p openwa-dll`
Expected: clean build

**Step 5: Commit**

```
feat: add Options__GetCrashReportURL hook
```

---

### Task 8: Full integration test + final commit

**Step 1: Build clean**

Run: `cargo build --release -p openwa-dll`
Expected: clean build, no warnings

**Step 2: Deploy**

```bash
cp target/i686-pc-windows-msvc/release/openwa.dll "I:/games/SteamLibrary/steamapps/common/Worms Armageddon/wkOpenWA.dll"
```

**Step 3: In-game verification**

1. Launch game → main menu
2. Check OpenWA.log for all 7 `[REPLACE]` messages + `[Config] LoadOptions completed`
3. Verify theme loads correctly (visual appearance unchanged)
4. Go to Options, change Detail Level, accept
5. Restart game, verify Detail Level persists
6. Verify existing scheme functionality still works (quick game setup)

**Step 4: Squash/rebase if desired, or leave incremental commits**
