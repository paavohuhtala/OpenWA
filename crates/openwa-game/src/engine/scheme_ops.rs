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
    EXTENDED_OPTIONS_DEFAULTS, EXTENDED_OPTIONS_SIZE, ExtendedOptions, SCHEME_PAYLOAD_V1,
    SCHEME_PAYLOAD_V2, SCHEME_PAYLOAD_V3, SchemeFile, SchemeVersion,
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

const PAYLOAD_SUPER_WEAPONS: usize = SCHEME_PAYLOAD_V1; // 0xD8
const SUPER_WEAPONS_SIZE: usize = SCHEME_PAYLOAD_V2 - SCHEME_PAYLOAD_V1; // 0x4C = 76
const PAYLOAD_EXTENDED: usize = SCHEME_PAYLOAD_V2; // 0x124

const SCHEME_SLOT_COUNT: u32 = 13;
const SCHEME_PE_RESOURCE_BASE: u32 = 0x2742;
const SCHEME_WEAPON_CHECK_COUNT: usize = 39;
const STRING_RES_DEFAULT_NAME: u32 = 0x0E;

// ─── Trampoline callback types ──────────────────────────────────────────────

pub type OriginalReadFile = unsafe extern "stdcall" fn(u32, u32, u32, u32) -> u32;
pub type OriginalSaveFile = unsafe extern "fastcall" fn(u32, u32, u32, u32) -> u32;

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Write a parsed SchemeFile into the WA dest struct at the correct offsets.
///
/// Replicates the memory writes from Scheme__ReadFile:
/// - V1: payload(0xD8) + zero super weapons(0x4C) + defaults(0x6E)
/// - V2: payload(0x124) + defaults(0x6E)
/// - V3: payload(0x192), with validation fallback to defaults
unsafe fn write_scheme_to_dest(scheme: &SchemeFile, dest: u32) {
    unsafe {
        let payload_ptr = (dest as usize + DEST_PAYLOAD) as *mut u8;

        match scheme.version {
            SchemeVersion::V1 => {
                core::ptr::copy_nonoverlapping(
                    scheme.payload.as_ptr(),
                    payload_ptr,
                    SCHEME_PAYLOAD_V1,
                );
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
                core::ptr::copy_nonoverlapping(
                    scheme.payload.as_ptr(),
                    payload_ptr,
                    SCHEME_PAYLOAD_V2,
                );
                core::ptr::copy_nonoverlapping(
                    EXTENDED_OPTIONS_DEFAULTS.as_ptr(),
                    payload_ptr.add(PAYLOAD_EXTENDED),
                    EXTENDED_OPTIONS_SIZE,
                );
            }
            SchemeVersion::V3 => {
                core::ptr::copy_nonoverlapping(
                    scheme.payload.as_ptr(),
                    payload_ptr,
                    SCHEME_PAYLOAD_V3,
                );
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
}

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
        match SchemeFile::from_file(path) {
            Ok(scheme) => {
                write_scheme_to_dest(&scheme, dest_struct);
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

/// usercall(ESI=dest, [stack]=output_ptr), RET 0x4. Returns 1=V1, 2=V2, 3=V3.
///
/// Determines scheme version from in-memory data by comparing extended options
/// against ROM defaults (V3 check) and checking super weapon slots (V1 vs V2).
pub unsafe fn detect_version(dest: u32, output_ptr: u32) -> u32 {
    unsafe {
        let defaults_base = rb(va::SCHEME_V3_DEFAULTS) as *const u8;

        // Scan extended options backwards (110 bytes at dest+0x138, vs ROM at 0x649AB8).
        // Original loops i from 0x6E down to 1, comparing dest[0x137+i] vs ROM_base[i]
        // where ROM_base = 0x649AB7 (so ROM_base[1] = 0x649AB8 = SCHEME_V3_DEFAULTS).
        for i in (1u32..=0x6E).rev() {
            let scheme_byte = *((dest + 0x137 + i) as *const u8);
            let default_byte = *defaults_base.add(i as usize - 1);
            if scheme_byte != default_byte {
                *(output_ptr as *mut u32) = i;
                return 3;
            }
        }

        // 19 super weapon slots at dest+0xEC; check bytes 0, 2, 3 of each 4-byte entry
        // (original: pcVar2 starts at dest+0xEE, checks pcVar2[-2], pcVar2[0], pcVar2[1]).
        for i in 0u32..19 {
            let base = dest + 0xEE + i * 4;
            if *((base - 2) as *const u8) != 0
                || *(base as *const u8) != 0
                || *((base + 1) as *const u8) != 0
            {
                return 2;
            }
        }

        1
    }
}

/// Helper for SaveFile: detect version from in-memory scheme struct.
unsafe fn detect_version_for_save(dest: u32) -> (u8, usize) {
    unsafe {
        let mut mismatch_offset: u32 = 0;
        let version = detect_version(dest, &mut mismatch_offset as *mut u32 as u32);
        match version {
            1 => (1, SCHEME_PAYLOAD_V1),
            2 => (2, SCHEME_PAYLOAD_V2),
            _ => (version as u8, mismatch_offset as usize + SCHEME_PAYLOAD_V2),
        }
    }
}

// ─── Scheme__SaveFile (0x4D44F0) ────────────────────────────────────────────

/// fastcall(this, name, flag) -> u32, RET 0x8. Returns 0 on success.
///
/// Detects scheme version from in-memory data, writes SCHM header + payload.
/// For V3, writes variable-length payload (only bytes differing from defaults).
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
        let (version_byte, payload_size) = detect_version_for_save(this);

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
        // Copy 0xD8 bytes (V1 payload) from src_data to dest+0x14.
        core::ptr::copy_nonoverlapping(
            src_data as *const u8,
            (dest + 0x14) as *mut u8,
            SCHEME_PAYLOAD_V1,
        );

        // Zero 0x4C bytes at dest+0xEC (super weapons area).
        core::ptr::write_bytes(
            (dest + 0xEC) as *mut u8,
            0,
            SCHEME_PAYLOAD_V2 - SCHEME_PAYLOAD_V1,
        );

        // Copy 110 bytes of V3 defaults from ROM to dest+0x138.
        let defaults = rb(va::SCHEME_V3_DEFAULTS) as *const u8;
        core::ptr::copy_nonoverlapping(defaults, (dest + 0x138) as *mut u8, EXTENDED_OPTIONS_SIZE);

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
