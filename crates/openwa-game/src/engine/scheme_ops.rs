//! Scheme file (.wsc) operations — Rust replacements for WA.exe `Scheme__*` functions.
//!
//! Cross-platform parsing primitives live in `openwa_core::scheme`; this module
//! adds the WA-specific glue: writing parsed schemes into WA's in-memory dest
//! struct, registry/PE-resource I/O for built-in extraction, MFC CString
//! interop in `init_from_data`, and global-pointer reads in `check_weapon_limits`.
//!
//! The DLL crate provides only trampolines + hook installation. For the two
//! operations that may fall back to the original WA function (`read_file` on
//! invalid UTF-8 input, `save_file` on I/O failure), the DLL passes its
//! MinHook trampoline as a callback.

use std::ffi::CStr;
use std::path::Path;

use openwa_core::log::log_line;
use openwa_core::scheme::{
    EXTENDED_OPTIONS_OFFSET, EXTENDED_OPTIONS_SIZE, ExtendedOptions, SCHEME_PAYLOAD_V1,
    SCHEME_PAYLOAD_V3, SUPER_WEAPONS_OFFSET, SUPER_WEAPONS_SIZE, Scheme, SchemeVersion,
};

use crate::address::va;
use crate::rebase::rb;
use crate::wa::mfc::{CStringRef, cstring_release};
use crate::wa::resource;

// ─── Public layout constants ────────────────────────────────────────────────

/// Dest struct offsets (WA runtime layout).
pub const DEST_FLAG: usize = 0x04;
pub const DEST_INDEX: usize = 0x08;
pub const DEST_NAME: usize = 0x0C;
pub const DEST_PAYLOAD: usize = 0x14;

/// Unassigned scheme index sentinel value.
pub const SCHEME_INDEX_UNASSIGNED: u32 = 0xFFFFFFFF;

const SCHEME_SLOT_COUNT: u32 = 13;
const SCHEME_PE_RESOURCE_BASE: u32 = 0x2742;
const SCHEME_WEAPON_CHECK_COUNT: usize = 39;
const STRING_RES_DEFAULT_NAME: u32 = 0x0E;

// ─── Trampoline callback types ──────────────────────────────────────────────

pub type OriginalReadFile = unsafe extern "stdcall" fn(u32, u32, u32, u32) -> u32;
pub type OriginalSaveFile = unsafe extern "fastcall" fn(u32, u32, u32, u32) -> u32;

// ─── Scheme__ReadFile (0x4D3890) ────────────────────────────────────────────

/// Equivalent to `Scheme__ReadFile(dest_struct, file_path, flag, out_ptr)`.
/// stdcall, RET 0x10. Returns 0 on success, `SCHEME_INDEX_UNASSIGNED` on Rust
/// file failure. Delegates to `original` on invalid UTF-8 in `file_path`.
///
/// `out_ptr` is an output bool: 0 = no gameplay modifiers applied, 1 = applied.
/// `flag` controls whether CheckWeaponLimits runs after loading.
///
/// TODO: Gameplay modifier globals (~20 fields at 0x88DBxx) are not yet ported.
/// These are applied in online lobbies; for now we always report "no modifiers".
pub unsafe fn read_file(
    dest_struct: u32,
    file_path: u32,
    flag: u32,
    out_ptr: u32,
    original: OriginalReadFile,
) -> u32 {
    unsafe {
        if out_ptr != 0 {
            *(out_ptr as *mut u8) = 0;
        }

        let name = if file_path != 0 {
            match CStr::from_ptr(file_path as *const i8).to_str() {
                Ok(s) => s,
                Err(_) => {
                    let _ =
                        log_line("[Scheme] ReadFile: invalid UTF-8 path, delegating to original");
                    return original(dest_struct, file_path, flag, out_ptr);
                }
            }
        } else {
            let _ = log_line("[Scheme] ReadFile: null path");
            return SCHEME_INDEX_UNASSIGNED;
        };

        let path = Path::new(name);
        match Scheme::from_file(path) {
            Ok(scheme) => {
                let payload = scheme.payload_bytes();
                core::ptr::copy_nonoverlapping(
                    payload.as_ptr(),
                    (dest_struct as usize + DEST_PAYLOAD) as *mut u8,
                    payload.len(),
                );
                *((dest_struct as usize + DEST_INDEX) as *mut u32) = SCHEME_INDEX_UNASSIGNED;
                *((dest_struct as usize + DEST_FLAG) as *mut u8) = 0;
                let _ = log_line(&format!(
                    "[Scheme] ReadFile OK: {name} -> canonical V3, dest=0x{dest_struct:08X}"
                ));
                0
            }
            Err(e) => {
                let _ = log_line(&format!("[Scheme] ReadFile FAILED (Rust): {name}: {e}"));
                SCHEME_INDEX_UNASSIGNED
            }
        }
    }
}

// ─── Scheme__ValidateExtendedOptions (0x4D5110) ─────────────────────────────

/// Validates the 110-byte extended-options block at `options_ptr`.
/// Returns 0 = valid, 1 = invalid.
pub unsafe fn validate_extended_options(options_ptr: u32) -> u32 {
    unsafe {
        let bytes = core::slice::from_raw_parts(options_ptr as *const u8, EXTENDED_OPTIONS_SIZE);
        if ExtendedOptions::validate_bytes(bytes) {
            0
        } else {
            1
        }
    }
}

// ─── Scheme__FileExists (0x4D4CD0) ──────────────────────────────────────────

/// stdcall(name) -> u32 (0 = not found, 1 = found).
/// Original formats `User\Schemes\%s.wsc` and opens with CFile; we use Path::exists.
pub unsafe fn file_exists(name: u32) -> u32 {
    unsafe {
        if name == 0 {
            return 0;
        }
        let c_name = CStr::from_ptr(name as *const i8);
        let name_str = c_name.to_string_lossy();
        let path = format!("User\\Schemes\\{name_str}.wsc");
        if Path::new(&path).exists() { 1 } else { 0 }
    }
}

// ─── Scheme__DetectVersion (0x4D4480) ───────────────────────────────────────

/// usercall(ESI=dest, [stack]=output_ptr), RET 0x4.
///
/// Schemes are canonicalized to V3 in memory, so version detection no longer
/// needs to scan for V1/V2-compatible truncation.
pub unsafe fn detect_version(_dest: u32, output_ptr: u32) -> u32 {
    unsafe {
        if output_ptr != 0 {
            *(output_ptr as *mut u32) = EXTENDED_OPTIONS_SIZE as u32;
        }
        SchemeVersion::V3 as u32
    }
}

// ─── Scheme__SaveFile (0x4D44F0) ────────────────────────────────────────────

/// fastcall(this, name, flag) -> u32, RET 0x8. Returns 0 on success.
///
/// Writes a full canonical V3 scheme file.
/// On invalid UTF-8 name or I/O failure, delegates to `original`.
pub unsafe fn save_file(this: u32, name: u32, flag: u32, original: OriginalSaveFile) -> u32 {
    unsafe {
        let c_name = match CStr::from_ptr(name as *const i8).to_str() {
            Ok(s) => s,
            Err(_) => {
                let _ = log_line("[Scheme] SaveFile: invalid UTF-8 name, delegating to original");
                return original(this, 0, name, flag);
            }
        };

        let path = format!("User\\Schemes\\{c_name}.wsc");

        let payload = core::slice::from_raw_parts(
            (this as usize + DEST_PAYLOAD) as *const u8,
            SCHEME_PAYLOAD_V3,
        );

        let save_result = Scheme::from_payload_bytes(payload)
            .map_err(|e| format!("parse runtime payload: {e}"))
            .and_then(|scheme| {
                scheme
                    .to_file(Path::new(&path))
                    .map_err(|e| format!("write file: {e}"))
            });

        match save_result {
            Ok(()) => {
                let _ = log_line(&format!(
                    "[Scheme] SaveFile OK (Rust): {c_name} -> canonical V3, {SCHEME_PAYLOAD_V3} payload bytes"
                ));
                0
            }
            Err(e) => {
                let _ = log_line(&format!("[Scheme] SaveFile FAILED (Rust): {path}: {e}"));
                original(this, 0, name, flag)
            }
        }
    }
}

// ─── Scheme__InitFromData (0x4D5020) ────────────────────────────────────────

/// fastcall(EDX=src_data, dest, name_cstring), RET 0x8.
///
/// Copies V1 payload data into scheme struct, zeroes super weapons,
/// applies V3 defaults from ROM, sets flag/index fields, assigns name.
pub unsafe fn init_from_data(src_data: u32, dest: u32, name_cstring: u32) {
    unsafe {
        let payload_dest = dest as usize + DEST_PAYLOAD;
        let super_weapons_dest = payload_dest + SUPER_WEAPONS_OFFSET;
        let extended_options_dest = payload_dest + EXTENDED_OPTIONS_OFFSET;

        // Copy the V1 payload from src_data into the runtime payload.
        core::ptr::copy_nonoverlapping(
            src_data as *const u8,
            payload_dest as *mut u8,
            SCHEME_PAYLOAD_V1,
        );

        // Zero the V2 super-weapons payload.
        core::ptr::write_bytes(super_weapons_dest as *mut u8, 0, SUPER_WEAPONS_SIZE);

        // Copy V3 extended-option defaults from ROM.
        let defaults = rb(va::SCHEME_V3_DEFAULTS) as *const u8;
        core::ptr::copy_nonoverlapping(
            defaults,
            extended_options_dest as *mut u8,
            EXTENDED_OPTIONS_SIZE,
        );

        *((dest + 0x04) as *mut u8) = 0;
        *((dest + 0x08) as *mut u32) = SCHEME_INDEX_UNASSIGNED;

        // CString name assignment: name_cstring is the char* data pointer of the
        // source CString; dest+DEST_NAME is the dest scheme's CString field.
        let src_len = *((name_cstring - 0x0C) as *const i32);
        let mut dest_name = CStringRef::new(dest + DEST_NAME as u32);
        if src_len == 0 {
            dest_name.assign_resource(STRING_RES_DEFAULT_NAME);
        } else {
            // operator= expects &CSimpleStringT (pointer to the char* pointer).
            // name_cstring is the char* itself, passed on the stack — its stack
            // address IS the CSimpleStringT object pointer.
            let src_name = CStringRef::new(&name_cstring as *const u32 as u32);
            dest_name.assign_from(&src_name);
        }

        cstring_release(name_cstring);
    }
}

// ─── Scheme__CheckWeaponLimits (0x4D50E0) ───────────────────────────────────

/// Compares 39 weapon bytes against ammo limit table.
/// Returns 0 = all within limits, 1 = at least one exceeds limit.
pub unsafe fn check_weapon_limits() -> u32 {
    unsafe {
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
}

// ─── Scheme__ScanDirectory (0x4D54E0) ───────────────────────────────────────

/// stdcall(cstring_param), RET 0x4.
///
/// Recursively scans `User\Schemes` for `{{DD}} name.wsc` files and marks
/// slot flags. The cstring_param is a char* data pointer from a CString
/// passed by the caller (ExtractBuiltins); we release its refcount.
pub unsafe fn scan_directory(cstring_param: u32) {
    unsafe {
        scan_directory_recursive("User\\Schemes");
        let _ = log_line("[Scheme] ScanDirectory completed (Rust)");
        cstring_release(cstring_param);
    }
}

/// Parse a scheme filename matching `{{DD}} name.wsc`. Returns slot index (1-13).
fn parse_scheme_slot(filename: &str) -> Option<u32> {
    let bytes = filename.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    if bytes[0] != b'{' || bytes[1] != b'{' {
        return None;
    }
    let d0 = bytes[2].wrapping_sub(b'0');
    let d1 = bytes[3].wrapping_sub(b'0');
    if d0 > 9 || d1 > 9 {
        return None;
    }
    if bytes[4] != b'}' || bytes[5] != b'}' {
        return None;
    }
    if !filename.ends_with(".wsc") {
        return None;
    }
    let slot = d0 as u32 * 10 + d1 as u32;
    // Original checks `slot - 1 < 0xD`.
    if (1..=SCHEME_SLOT_COUNT).contains(&slot) {
        Some(slot)
    } else {
        None
    }
}

unsafe fn scan_directory_recursive(dir: &str) {
    unsafe {
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
}

// ─── Scheme__ExtractBuiltins (0x4D5720) ─────────────────────────────────────

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

/// Zeros slot flags, scans for existing scheme files, then extracts missing
/// built-in schemes from PE resources (type "SCHEMES") to `User\Schemes\`.
pub unsafe fn extract_builtins() {
    unsafe {
        // Zero 16 bytes of slot flags (4 DWORDs covering slots 0-15).
        let slot_flags = rb(va::SCHEME_SLOT_FLAGS) as *mut u8;
        core::ptr::write_bytes(slot_flags, 0, 16);

        let _ = std::fs::create_dir_all("User\\Schemes");
        scan_directory_recursive("User\\Schemes");

        for slot in 1..=SCHEME_SLOT_COUNT {
            if *slot_flags.add(slot as usize) != 0 {
                continue;
            }

            // Slot 13: original has an obfuscated feature check (Scheme__Slot13Check) using
            // __usercall (implicit EAX/ECX). On Steam copies all slots are available,
            // so we skip the check.

            let name = BUILTIN_SCHEME_NAMES[slot as usize];
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
    }
}
