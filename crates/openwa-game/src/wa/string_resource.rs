//! Symbolic names for WA.exe string resources and a Rust port of
//! `WA__LoadStringResource` (0x00593180).
//!
//! The generated `StringRes` newtype, the `res::*` constant module, and the
//! `NAMES` lookup table come from `build.rs` reading
//! `data/string_resources.txt` (the extracted `g_LocalizationKeyTable`).

use core::ffi::c_char;

use crate::address::va;
use crate::rebase::rb;

include!(concat!(env!("OUT_DIR"), "/string_resource.rs"));

/// Localization data record as seen by `WA__LoadStringResource`.
///
/// Two globals of this type exist: `g_LocalizationData_Primary` and
/// `g_LocalizationData_Secondary`. Each holds a base pointer plus a per-entry
/// offset array; an entry with offset 0 means "not overridden, fall through".
#[repr(C)]
pub struct LocalizationData {
    _unknown_00: u32,
    /// Base pointer into a string blob; `base + offsets[i]` yields the
    /// localized C string for entry `i`.
    pub base: *const c_char,
    /// Per-entry offset table, indexed by `StringRes::as_offset()`.
    /// Zero means "no override, use the default key table".
    pub offsets: *const i32,
}

/// Rust port of `WA__LoadStringResource` (0x00593180, stdcall).
///
/// Tries the secondary localization record, then the primary one; falls back
/// to the default `g_LocalizationKeyTable` entry. Matches the original
/// check order exactly.
pub unsafe fn wa_load_string(id: StringRes) -> *const c_char {
    unsafe { load_string_raw(id.as_offset()) }
}

/// Raw offset variant used by the full-replacement hook. WA.exe callers pass
/// arbitrary `u32` values; we preserve the original out-of-range behavior
/// (which reads past the tables) rather than validate.
#[inline]
unsafe fn load_string_raw(id: u32) -> *const c_char {
    unsafe {
        for slot in [
            va::G_LOCALIZATION_DATA_SECONDARY,
            va::G_LOCALIZATION_DATA_PRIMARY,
        ] {
            let data = *(rb(slot) as *const *const LocalizationData);
            if data.is_null() {
                continue;
            }
            let off = *(*data).offsets.add(id as usize);
            if off != 0 {
                return (*data).base.wrapping_offset(off as isize);
            }
        }
        *(rb(va::G_LOCALIZATION_KEY_TABLE) as *const *const c_char).add(id as usize)
    }
}

/// Full-replacement detour for WA_LOAD_STRING. Stdcall(1).
pub unsafe extern "stdcall" fn wa_load_string_detour(id: u32) -> *const c_char {
    unsafe { load_string_raw(id) }
}
