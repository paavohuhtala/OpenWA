//! Scheme file (.wsc) hooks.
//!
//! Replaces several WA.exe Scheme__ functions with Rust implementations:
//! - Scheme__ReadFile (0x4D3890): full Rust file I/O using SchemeFile parser
//! - Scheme__ValidateExtendedOptions (0x4D5110): pure Rust validation
//! - Scheme__FileExists (0x4D4CD0): Rust path check
//! - Scheme__CheckWeaponLimits (0x4D50E0): weapon limit validation
//! - Scheme__DetectVersion (0x4D4480): version detection from in-memory scheme
//! - Scheme__SaveFile (0x4D44F0): file write with Rust I/O
//! - Scheme__InitFromData (0x4D5020): scheme struct initialization
//! - Scheme__ScanDirectory (0x4D54E0): directory scan for numbered schemes

use std::ffi::CStr;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::log_line;
use openwa_core::address::va;
use openwa_core::game::scheme::{
    ExtendedOptions, SchemeFile, SchemeVersion, EXTENDED_OPTIONS_DEFAULTS, EXTENDED_OPTIONS_SIZE,
    SCHEME_PAYLOAD_V1, SCHEME_PAYLOAD_V2, SCHEME_PAYLOAD_V3,
};
use openwa_core::rebase::rb;

// ============================================================
// Scheme__ReadFile replacement (0x4D3890)
// ============================================================

/// Trampoline to original Scheme__ReadFile (for fallback on flag/out_ptr path).
static ORIG_SCHEME_READ_FILE: AtomicU32 = AtomicU32::new(0);

/// Dest struct offsets (WA runtime layout).
const DEST_FLAG: usize = 0x04;
const DEST_INDEX: usize = 0x08;
const DEST_NAME: usize = 0x0C;
const DEST_PAYLOAD: usize = 0x14;
/// Offset within payload where super weapons start (V1 payload ends here).
const PAYLOAD_SUPER_WEAPONS: usize = SCHEME_PAYLOAD_V1; // 0xD8
/// Size of super weapons region to zero for V1 schemes.
const SUPER_WEAPONS_SIZE: usize = SCHEME_PAYLOAD_V2 - SCHEME_PAYLOAD_V1; // 0x4C = 76
/// Offset within payload where extended options start.
const PAYLOAD_EXTENDED: usize = SCHEME_PAYLOAD_V2; // 0x124

/// Unassigned scheme index sentinel value.
const SCHEME_INDEX_UNASSIGNED: u32 = 0xFFFF_FFFF;
/// Number of numbered scheme slots (1-13).
const SCHEME_SLOT_COUNT: u32 = 13;
/// PE resource ID base for built-in scheme data.
const SCHEME_PE_RESOURCE_BASE: u32 = 0x2742;
/// Number of weapons checked by CheckWeaponLimits.
const SCHEME_WEAPON_CHECK_COUNT: usize = 39;
/// MFC string resource ID for default scheme name.
const STRING_RES_DEFAULT_NAME: u32 = 0x0E;

/// Write a parsed SchemeFile into the WA dest struct at the correct offsets.
///
/// Replicates the memory writes from Scheme__ReadFile:
/// - V1: payload(0xD8) + zero super weapons(0x4C) + defaults(0x6E)
/// - V2: payload(0x124) + defaults(0x6E)
/// - V3: payload(0x192), with validation fallback to defaults
unsafe fn write_scheme_to_dest(scheme: &SchemeFile, dest: u32) {
    let payload_ptr = (dest as usize + DEST_PAYLOAD) as *mut u8;

    match scheme.version {
        SchemeVersion::V1 => {
            core::ptr::copy_nonoverlapping(scheme.payload.as_ptr(), payload_ptr, SCHEME_PAYLOAD_V1);
            core::ptr::write_bytes(
                payload_ptr.add(PAYLOAD_SUPER_WEAPONS),
                0,
                SUPER_WEAPONS_SIZE,
            );
            core::ptr::copy_nonoverlapping(
                EXTENDED_OPTIONS_DEFAULTS.as_ptr(),
                payload_ptr.add(PAYLOAD_EXTENDED),
                EXTENDED_OPTIONS_SIZE,
            );
        }
        SchemeVersion::V2 => {
            core::ptr::copy_nonoverlapping(scheme.payload.as_ptr(), payload_ptr, SCHEME_PAYLOAD_V2);
            core::ptr::copy_nonoverlapping(
                EXTENDED_OPTIONS_DEFAULTS.as_ptr(),
                payload_ptr.add(PAYLOAD_EXTENDED),
                EXTENDED_OPTIONS_SIZE,
            );
        }
        SchemeVersion::V3 => {
            core::ptr::copy_nonoverlapping(scheme.payload.as_ptr(), payload_ptr, SCHEME_PAYLOAD_V3);
            let ext_bytes = &scheme.payload[PAYLOAD_EXTENDED..];
            if !ExtendedOptions::validate_bytes(ext_bytes) {
                core::ptr::copy_nonoverlapping(
                    EXTENDED_OPTIONS_DEFAULTS.as_ptr(),
                    payload_ptr.add(PAYLOAD_EXTENDED),
                    EXTENDED_OPTIONS_SIZE,
                );
            }
        }
    }

    *((dest as usize + DEST_INDEX) as *mut u32) = SCHEME_INDEX_UNASSIGNED;
}

/// Rust replacement for Scheme__ReadFile (0x4D3890).
/// stdcall(dest_struct, file_path, flag, out_ptr) -> u32, RET 0x10
///
/// out_ptr is an output bool: 0 = no gameplay modifiers applied, 1 = modifiers applied.
/// flag controls whether CheckWeaponLimits is called after loading.
///
/// TODO: Gameplay modifier globals (~20 fields at 0x88DBxx) are not yet ported.
/// These are applied in online lobbies; for now we always report "no modifiers applied".
unsafe extern "stdcall" fn hook_scheme_read_file(
    dest_struct: u32,
    file_path: u32,
    _flag: u32,
    out_ptr: u32,
) -> u32 {
    // Initialize out_ptr to 0 (no modifiers applied), matching original behavior
    if out_ptr != 0 {
        *(out_ptr as *mut u8) = 0;
    }

    // Read the path string
    let name = if file_path != 0 {
        match CStr::from_ptr(file_path as *const i8).to_str() {
            Ok(s) => s,
            Err(_) => {
                let _ = log_line("[Scheme] ReadFile: invalid UTF-8 path, delegating to original");
                return call_original_read_file(dest_struct, file_path, _flag, out_ptr);
            }
        }
    } else {
        let _ = log_line("[Scheme] ReadFile: null path");
        return SCHEME_INDEX_UNASSIGNED;
    };

    // Use Rust parser for file I/O
    let path = Path::new(name);
    match SchemeFile::from_file(path) {
        Ok(scheme) => {
            write_scheme_to_dest(&scheme, dest_struct);

            // Set flag byte to 0 and scheme index to 0xFFFFFFFF
            *((dest_struct as usize + DEST_FLAG) as *mut u8) = 0;

            let _ = log_line(&format!(
                "[Scheme] ReadFile OK: {name} -> {:?}, dest=0x{dest_struct:08X}",
                scheme.version
            ));
            0
        }
        Err(e) => {
            let _ = log_line(&format!("[Scheme] ReadFile FAILED (Rust): {name}: {e}"));
            SCHEME_INDEX_UNASSIGNED
        }
    }
}

/// Call the original Scheme__ReadFile via trampoline.
#[inline]
unsafe fn call_original_read_file(dest: u32, path: u32, flag: u32, out_ptr: u32) -> u32 {
    let orig: unsafe extern "stdcall" fn(u32, u32, u32, u32) -> u32 =
        core::mem::transmute(ORIG_SCHEME_READ_FILE.load(Ordering::Relaxed));
    orig(dest, path, flag, out_ptr)
}

// ============================================================
// Scheme__ValidateExtendedOptions replacement (0x4D5110)
// ============================================================

// Naked trampoline: captures EAX (110-byte extended options pointer). Returns 0=valid, 1=invalid.
crate::hook::usercall_trampoline!(fn trampoline_validate_ext_opts;
    impl_fn = validate_extended_options_impl; reg = eax);

/// Rust implementation called by the naked trampoline.
unsafe extern "cdecl" fn validate_extended_options_impl(options_ptr: u32) -> u32 {
    let bytes = core::slice::from_raw_parts(options_ptr as *const u8, EXTENDED_OPTIONS_SIZE);
    if ExtendedOptions::validate_bytes(bytes) {
        0 // valid
    } else {
        1 // invalid
    }
}

// ============================================================
// Scheme__FileExists replacement (0x4D4CD0)
// ============================================================

/// Rust replacement for Scheme__FileExists (0x4D4CD0).
/// stdcall(name) -> u32 (0 = not found, 1 = found), RET 0x4
///
/// Original: formats "User\Schemes\%s.wsc", opens with CFile.
/// Rust: uses std::path::Path::exists().
unsafe extern "stdcall" fn hook_scheme_file_exists(name: u32) -> u32 {
    if name == 0 {
        return 0;
    }
    let c_name = CStr::from_ptr(name as *const i8);
    let name_str = c_name.to_string_lossy();
    let path = format!("User\\Schemes\\{name_str}.wsc");
    if Path::new(&path).exists() {
        1
    } else {
        0
    }
}

// ============================================================
// Scheme__DetectVersion replacement (0x4D4480)
// ============================================================

// Naked trampoline: captures ESI (dest struct pointer) + 1 stack arg (output pointer).
// Returns 1=V1, 2=V2, 3=V3. RET 0x4.
crate::hook::usercall_trampoline!(fn trampoline_detect_version;
    impl_fn = detect_version_impl; reg = esi;
    stack_params = 1; ret_bytes = "0x4");

/// Rust implementation of Scheme__DetectVersion.
///
/// Determines scheme version from in-memory data by comparing extended options
/// against ROM defaults (V3 check) and checking super weapon slots (V1 vs V2).
unsafe extern "cdecl" fn detect_version_impl(dest: u32, output_ptr: u32) -> u32 {
    let defaults_base = rb(va::SCHEME_V3_DEFAULTS) as *const u8;

    // Scan extended options backwards (110 bytes at dest+0x138, compared to ROM at 0x649AB8)
    // Original loops i from 0x6E down to 1 (not 0), comparing dest[0x137+i] vs ROM_base[i]
    // where ROM_base = 0x649AB7 (so ROM_base[1] = 0x649AB8 = SCHEME_V3_DEFAULTS)
    for i in (1u32..=0x6E).rev() {
        let scheme_byte = *((dest + 0x137 + i) as *const u8);
        // defaults_base points to 0x649AB8 = ROM_base+1, so defaults_base[i-1] = ROM_base[i]
        let default_byte = *defaults_base.add(i as usize - 1);
        if scheme_byte != default_byte {
            *(output_ptr as *mut u32) = i;
            return 3; // V3
        }
    }

    // Check 19 super weapon slots at dest+0xEC, checking bytes 0, 2, 3 of each 4-byte entry
    // Original: pcVar2 starts at dest+0xEE, checks pcVar2[-2], pcVar2[0], pcVar2[1]
    for i in 0u32..19 {
        let base = dest + 0xEE + i * 4;
        if *((base - 2) as *const u8) != 0
            || *(base as *const u8) != 0
            || *((base + 1) as *const u8) != 0
        {
            return 2; // V2
        }
    }

    1 // V1
}

/// Helper for SaveFile: detect version from in-memory scheme struct.
/// Returns (version_byte, payload_size).
unsafe fn detect_version_for_save(dest: u32) -> (u8, usize) {
    let mut mismatch_offset: u32 = 0;
    let version = detect_version_impl(dest, &mut mismatch_offset as *mut u32 as u32);
    match version {
        1 => (1, SCHEME_PAYLOAD_V1),
        2 => (2, SCHEME_PAYLOAD_V2),
        _ => (version as u8, mismatch_offset as usize + SCHEME_PAYLOAD_V2),
    }
}

// ============================================================
// Scheme__SaveFile replacement (0x4D44F0)
// ============================================================

/// Trampoline to original Scheme__SaveFile (for error path delegation).
static ORIG_SCHEME_SAVE_FILE: AtomicU32 = AtomicU32::new(0);

/// Rust replacement for Scheme__SaveFile (0x4D44F0).
/// thiscall(this, name, flag) -> u32, RET 0x8
///
/// Detects scheme version from in-memory data, writes SCHM header + payload.
/// For V3, writes variable-length payload (only bytes differing from defaults).
unsafe extern "fastcall" fn hook_save_file(this: u32, _edx: u32, name: u32, flag: u32) -> u32 {
    let c_name = match CStr::from_ptr(name as *const i8).to_str() {
        Ok(s) => s,
        Err(_) => {
            let _ = log_line("[Scheme] SaveFile: invalid UTF-8 name, delegating to original");
            return call_original_save_file(this, name, flag);
        }
    };

    let path = format!("User\\Schemes\\{c_name}.wsc");

    // Detect version from in-memory scheme data
    let (version_byte, payload_size) = detect_version_for_save(this);

    // Build file contents
    let mut buf = Vec::with_capacity(5 + payload_size);
    buf.extend_from_slice(b"SCHM");
    buf.push(version_byte);
    let payload = core::slice::from_raw_parts((this + 0x14) as *const u8, payload_size);
    buf.extend_from_slice(payload);

    match std::fs::write(&path, &buf) {
        Ok(()) => {
            let _ = log_line(&format!(
                "[Scheme] SaveFile OK (Rust): {c_name} -> V{version_byte}, {payload_size} bytes"
            ));
            0
        }
        Err(e) => {
            let _ = log_line(&format!("[Scheme] SaveFile FAILED (Rust): {path}: {e}"));
            // Delegate to original for error handling (flag==0 shows error dialog)
            call_original_save_file(this, name, flag)
        }
    }
}

/// Call the original Scheme__SaveFile via trampoline.
#[inline]
unsafe fn call_original_save_file(this: u32, name: u32, flag: u32) -> u32 {
    let orig: unsafe extern "fastcall" fn(u32, u32, u32, u32) -> u32 =
        core::mem::transmute(ORIG_SCHEME_SAVE_FILE.load(Ordering::Relaxed));
    orig(this, 0, name, flag)
}

// ============================================================
// Scheme__InitFromData replacement (0x4D5020)
// ============================================================

/// Rust replacement for Scheme__InitFromData (0x4D5020).
/// fastcall(ECX=unused, EDX=src_data, dest, name_cstring), RET 0x8
///
/// Copies V1 payload data into scheme struct, zeroes super weapons,
/// applies V3 defaults from ROM, sets flag/index fields, assigns name.
unsafe extern "fastcall" fn hook_init_from_data(
    _ecx: u32,
    src_data: u32,
    dest: u32,
    name_cstring: u32,
) {
    use openwa_core::wa::mfc::{cstring_release, CStringRef};

    // Step 1: Copy 0xD8 bytes (V1 payload) from src_data to dest+0x14
    core::ptr::copy_nonoverlapping(
        src_data as *const u8,
        (dest + 0x14) as *mut u8,
        SCHEME_PAYLOAD_V1,
    );

    // Step 2: Zero 0x4C bytes at dest+0xEC (super weapons area)
    core::ptr::write_bytes(
        (dest + 0xEC) as *mut u8,
        0,
        SCHEME_PAYLOAD_V2 - SCHEME_PAYLOAD_V1, // 0x4C = 76
    );

    // Step 3: Copy 110 bytes of V3 defaults from ROM to dest+0x138
    let defaults = rb(va::SCHEME_V3_DEFAULTS) as *const u8;
    core::ptr::copy_nonoverlapping(defaults, (dest + 0x138) as *mut u8, EXTENDED_OPTIONS_SIZE);

    // Step 4: Set flag byte and index
    *((dest + 0x04) as *mut u8) = 0;
    *((dest + 0x08) as *mut u32) = SCHEME_INDEX_UNASSIGNED;

    // Step 5: CString name assignment.
    // name_cstring is the char* data pointer of the source CString.
    // dest+DEST_NAME is the CString field in the dest scheme struct.
    let src_len = *((name_cstring - 0x0C) as *const i32);
    let mut dest_name = CStringRef::new(dest + DEST_NAME as u32);
    if src_len == 0 {
        dest_name.assign_resource(STRING_RES_DEFAULT_NAME);
    } else {
        // Non-empty: copy via CString operator=.
        // operator= expects &CSimpleStringT (pointer to the char* pointer).
        // name_cstring is the char* itself, passed on the stack — its stack
        // address IS the CSimpleStringT object pointer.
        let src_name = CStringRef::new(&name_cstring as *const u32 as u32);
        dest_name.assign_from(&src_name);
    }

    // Step 6: Release the source CString (original decrements refcount at end).
    cstring_release(name_cstring);
}

// ============================================================
// Scheme__CheckWeaponLimits replacement (0x4D50E0)
// ============================================================

/// Rust replacement for Scheme__CheckWeaponLimits (0x4D50E0).
/// No params, plain ret. Compares 39 weapon bytes against ammo limit table.
/// Returns 0 = all within limits, 1 = at least one exceeds limit.
unsafe extern "stdcall" fn hook_check_weapon_limits() -> u32 {
    let limits = rb(va::SCHEME_WEAPON_AMMO_LIMITS) as *const u8;
    let weapons = rb(va::SCHEME_ACTIVE_WEAPON_DATA) as *const u8;
    for i in 0..SCHEME_WEAPON_CHECK_COUNT {
        let limit = *limits.add(i);
        let current = *weapons.add(i * 4);
        if limit <= current {
            return 1;
        }
    }
    0
}

// ============================================================
// Scheme__ScanDirectory replacement (0x4D54E0)
// ============================================================

/// Rust replacement for Scheme__ScanDirectory (0x4D54E0).
/// stdcall(cstring_param), RET 0x4
///
/// Recursively scans for {{DD}} name.wsc files and marks slot flags.
/// The cstring_param is a char* data pointer from a CString passed by the caller
/// (ExtractBuiltins). We must release its refcount when done.
unsafe extern "stdcall" fn hook_scan_directory(cstring_param: u32) {
    // Perform the Rust directory scan
    scan_directory_recursive("User\\Schemes");

    let _ = log_line("[Scheme] ScanDirectory completed (Rust)");

    // Release the CString parameter's refcount (caller transferred ownership)
    openwa_core::wa::mfc::cstring_release(cstring_param);
}

/// Parse a scheme filename matching the pattern `{{DD}} name.wsc`.
/// Returns the slot index (1-13) if valid, None otherwise.
fn parse_scheme_slot(filename: &str) -> Option<u32> {
    let bytes = filename.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    // Must start with {{ and have digits at positions 2-3
    if bytes[0] != b'{' || bytes[1] != b'{' {
        return None;
    }
    let d0 = bytes[2].wrapping_sub(b'0');
    let d1 = bytes[3].wrapping_sub(b'0');
    if d0 > 9 || d1 > 9 {
        return None;
    }
    // Must have }} after digits
    if bytes[4] != b'}' || bytes[5] != b'}' {
        return None;
    }
    // Must end with .wsc (case-sensitive, matching original)
    if !filename.ends_with(".wsc") {
        return None;
    }
    let slot = d0 as u32 * 10 + d1 as u32;
    // Valid range: 1-13 (original checks `slot - 1 < 0xD`)
    if (1..=SCHEME_SLOT_COUNT).contains(&slot) {
        Some(slot)
    } else {
        None
    }
}

/// Recursively scan a directory for numbered scheme files.
unsafe fn scan_directory_recursive(dir: &str) {
    let slot_flags = rb(va::SCHEME_SLOT_FLAGS) as *mut u8;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if file_type.is_file() {
            if let Some(slot) = parse_scheme_slot(&name_str) {
                *slot_flags.add(slot as usize) = 1;
            }
        } else if file_type.is_dir() && name_str != "." && name_str != ".." {
            let subdir = format!("{dir}\\{name_str}");
            scan_directory_recursive(&subdir);
        }
    }
}

// ============================================================
// Scheme__ExtractBuiltins replacement (0x4D5720)
// ============================================================

/// Built-in scheme names, indexed by slot (1-13).
/// Original loads these from MFC string resources (IDs 0x3CA-0x3D6) via
/// AfxFindStringResourceHandle, which is not accessible via plain LoadStringA.
/// The names are fixed across all WA 3.8.1 installations.
const BUILTIN_SCHEME_NAMES: [&str; 14] = [
    "",               // slot 0 unused
    "Beginner",       // slot 1, resource 0x3CA
    "Intermediate",   // slot 2, resource 0x3CB
    "Pro",            // slot 3, resource 0x3CC
    "Tournament",     // slot 4, resource 0x3D1
    "Classic",        // slot 5, resource 0x3CD
    "Retro",          // slot 6, resource 0x3D2
    "Artillery",      // slot 7, resource 0x3D5
    "Sudden Sinking", // slot 8, resource 0x3D0
    "Strategic",      // slot 9, resource 0x3D3
    "The Darkside",   // slot 10, resource 0x3D4
    "Armageddon",     // slot 11, resource 0x3CE
    "Blast Zone",     // slot 12, resource 0x3CF
    "Full Wormage",   // slot 13, resource 0x3D6
];

/// Rust replacement for Scheme__ExtractBuiltins (0x4D5720).
/// No params, no return value. Called from Frontend__MainNavigationLoop at startup.
///
/// Zeros slot flags, scans for existing scheme files, then extracts missing
/// built-in schemes from PE resources (type "SCHEMES") to User\Schemes\.
unsafe extern "stdcall" fn hook_extract_builtins() {
    use openwa_core::wa::resource;

    // Step 1: Zero 16 bytes of slot flags (4 DWORDs covering slots 0-15)
    let slot_flags = rb(va::SCHEME_SLOT_FLAGS) as *mut u8;
    core::ptr::write_bytes(slot_flags, 0, 16);

    // Step 2: Ensure directory exists
    let _ = std::fs::create_dir_all("User\\Schemes");

    // Step 3: Scan for existing scheme files (reuse our Rust implementation)
    scan_directory_recursive("User\\Schemes");

    // Step 4: Extract missing built-in schemes
    for slot in 1..=SCHEME_SLOT_COUNT {
        if *slot_flags.add(slot as usize) != 0 {
            continue; // file already exists
        }

        // Slot 13: original has an obfuscated feature check (FUN_004DA4C0) that
        // uses __usercall (implicit EAX/ECX). On Steam copies all slots are available,
        // so we skip the check. If needed, this could be replicated later.

        let name = BUILTIN_SCHEME_NAMES[slot as usize];

        // Format output path: User\Schemes\{{DD}} name.wsc
        let path = format!("User\\Schemes\\{{{{{slot:02}}}}} {name}.wsc");

        let resource_id = slot + SCHEME_PE_RESOURCE_BASE;
        match resource::load_pe_resource("SCHEMES", resource_id) {
            Some(data) => {
                if let Err(e) = std::fs::write(&path, data) {
                    let _ = log_line(&format!(
                        "[Scheme] ExtractBuiltins: failed to write {path}: {e}"
                    ));
                }
            }
            None => {
                let _ = log_line(&format!(
                    "[Scheme] ExtractBuiltins: PE resource 0x{resource_id:X} not found for slot {slot}"
                ));
            }
        }
    }

    let _ = log_line("[Scheme] ExtractBuiltins completed (Rust)");
}

// ============================================================
// Scheme__LoadNumbered logging sentinel (0x4D4E00)
// ============================================================

/// Trampoline to original Scheme__LoadNumbered.
static ORIG_LOAD_NUMBERED: AtomicU32 = AtomicU32::new(0);

/// Logging sentinel for Scheme__LoadNumbered (0x4D4E00).
/// This function has zero xrefs in WA.exe (believed dead code).
/// We hook it to detect if it's actually called at runtime.
unsafe extern "stdcall" fn hook_load_numbered(name: u32) -> u32 {
    let c_name = CStr::from_ptr(name as *const i8).to_str().unwrap_or("???");
    let _ = log_line(&format!(
        "[Scheme] WARNING: LoadNumbered called (believed dead code): {c_name}"
    ));
    let orig: unsafe extern "stdcall" fn(u32) -> u32 =
        core::mem::transmute(ORIG_LOAD_NUMBERED.load(Ordering::Relaxed));
    orig(name)
}

// ============================================================
// Hook installation
// ============================================================

pub fn install() -> Result<(), String> {
    unsafe {
        let trampoline = crate::hook::install(
            "Scheme__ReadFile",
            va::SCHEME_READ_FILE,
            hook_scheme_read_file as *const (),
        )?;
        ORIG_SCHEME_READ_FILE.store(trampoline as u32, Ordering::Relaxed);

        let _ = crate::hook::install(
            "Scheme__ValidateExtendedOptions",
            va::SCHEME_VALIDATE_EXTENDED_OPTIONS,
            trampoline_validate_ext_opts as *const (),
        )?;

        let _ = crate::hook::install(
            "Scheme__FileExists",
            va::SCHEME_FILE_EXISTS,
            hook_scheme_file_exists as *const (),
        )?;

        let _ = crate::hook::install(
            "Scheme__CheckWeaponLimits",
            va::SCHEME_CHECK_WEAPON_LIMITS,
            hook_check_weapon_limits as *const (),
        )?;

        let _ = crate::hook::install(
            "Scheme__DetectVersion",
            va::SCHEME_DETECT_VERSION,
            trampoline_detect_version as *const (),
        )?;

        let trampoline_save = crate::hook::install(
            "Scheme__SaveFile",
            va::SCHEME_SAVE_FILE,
            hook_save_file as *const (),
        )?;
        ORIG_SCHEME_SAVE_FILE.store(trampoline_save as u32, Ordering::Relaxed);

        let _ = crate::hook::install(
            "Scheme__InitFromData",
            va::SCHEME_INIT_FROM_DATA,
            hook_init_from_data as *const (),
        )?;

        let _ = crate::hook::install(
            "Scheme__ScanDirectory",
            va::SCHEME_SCAN_DIRECTORY,
            hook_scan_directory as *const (),
        )?;

        let _ = crate::hook::install(
            "Scheme__ExtractBuiltins",
            va::SCHEME_EXTRACT_BUILTINS,
            hook_extract_builtins as *const (),
        )?;

        let trampoline_load = crate::hook::install(
            "Scheme__LoadNumbered",
            va::SCHEME_FILE_EXISTS_NUMBERED,
            hook_load_numbered as *const (),
        )?;
        ORIG_LOAD_NUMBERED.store(trampoline_load as u32, Ordering::Relaxed);
    }

    Ok(())
}
