//! Rust port of WA's `InitWeaponNameStrings` (0x0053C130).
//!
//! Populates the `name1` / `name2` string-pointer pair on every
//! [`WeaponEntry`] in the active [`WeaponTable`]. Three phases:
//!
//! 1. Default every entry to the localized `ERROR_L` string (StringRes
//!    id 0x1C) — the "this weapon name is missing" placeholder.
//! 2. Overwrite entry 0's pair with the localized `NONE` string (id 0x11),
//!    falling back to the literal ASCII `"NONE"` at [`va::PTR_S_NONE`]
//!    when neither localization record supplies it. (WA's
//!    `WA__LoadStringResource` would fall back to the key table here;
//!    this entry-0 path uses the hard-coded literal instead.)
//! 3. For each weapon in [`WEAPON_NAME_TOKENS`], resolve a `(name2, name1)`
//!    pair through [`resolve_raw`] and write them into the entry.

use core::ffi::c_char;

use crate::address::va;
use crate::engine::world::GameWorld;
use crate::rebase::rb;
use crate::wa::localized_template::resolve_raw;
use crate::wa::string_resource::{LocalizationData, StringRes, wa_load_string};

/// StringRes id for `ERROR_L` — the "missing localization" placeholder
/// that WA assigns to every weapon entry as a default.
const PLACEHOLDER_NAME: StringRes = StringRes::new_unchecked(0x1C);

/// Offset into [`LocalizationData::offsets`] for the `NONE` string
/// (id 0x11 × 4 = 0x44 bytes). Mirrors WA's hard-coded constant.
const NONE_OFFSET_INDEX: usize = 0x11;

/// `(name2_token, name1_token)` pairs for entries 1..=70. The second
/// token is always one greater than the first *except* where WA's
/// localization table reserves intermediate ids for hold-key variants
/// (e.g. between entries 26 and 27, 31 and 32, 43 and 44, 56 and 57,
/// and the trailing super-weapon block 62..=70 which uses ids 0x3F..0x59).
const WEAPON_NAME_TOKENS: [(u32, u32); 70] = [
    (0x5A, 0x5B),   // 1
    (0x5D, 0x5E),   // 2
    (0x60, 0x61),   // 3
    (0x63, 0x64),   // 4
    (0x67, 0x68),   // 5
    (0x6B, 0x6C),   // 6
    (0x6E, 0x6F),   // 7
    (0x71, 0x72),   // 8
    (0x74, 0x75),   // 9
    (0x77, 0x78),   // 10
    (0x7C, 0x7D),   // 11
    (0x7F, 0x80),   // 12
    (0x82, 0x83),   // 13
    (0x85, 0x86),   // 14
    (0x88, 0x89),   // 15
    (0x8B, 0x8C),   // 16
    (0x8E, 0x8F),   // 17
    (0x91, 0x92),   // 18
    (0x94, 0x95),   // 19
    (0x97, 0x98),   // 20
    (0x9A, 0x9B),   // 21
    (0x9D, 0x9E),   // 22
    (0xA0, 0xA1),   // 23
    (0xA3, 0xA4),   // 24
    (0xA7, 0xA8),   // 25
    (0xA9, 0xAA),   // 26
    (0xAD, 0xAE),   // 27
    (0xB0, 0xB1),   // 28
    (0xB3, 0xB4),   // 29
    (0xB6, 0xB7),   // 30
    (0xB9, 0xBA),   // 31
    (0xC0, 0xC1),   // 32
    (0xC3, 0xC4),   // 33
    (0xC6, 0xC7),   // 34
    (0xC9, 0xCA),   // 35
    (0xCC, 0xCD),   // 36
    (0xCF, 0xD0),   // 37
    (0xD2, 0xD3),   // 38
    (0xD5, 0xD6),   // 39
    (0xD8, 0xD9),   // 40
    (0xDB, 0xDC),   // 41
    (0xDE, 0xDF),   // 42
    (0xE1, 0xE2),   // 43
    (0xE5, 0xE6),   // 44
    (0xE9, 0xEA),   // 45
    (0xEC, 0xED),   // 46
    (0xEF, 0xF0),   // 47
    (0xF2, 0xF3),   // 48
    (0xF5, 0xF6),   // 49
    (0xF8, 0xF9),   // 50
    (0xFB, 0xFC),   // 51
    (0xFE, 0xFF),   // 52
    (0x102, 0x103), // 53
    (0x106, 0x107), // 54
    (0x109, 0x10A), // 55
    (0x10C, 0x10D), // 56
    (0x111, 0x112), // 57
    (0x113, 0x114), // 58
    (0x115, 0x116), // 59
    (0x118, 0x119), // 60
    (0x11B, 0x11C), // 61
    (0x3F, 0x40),   // 62
    (0x42, 0x43),   // 63
    (0x45, 0x46),   // 64
    (0x48, 0x49),   // 65
    (0x4B, 0x4C),   // 66
    (0x55, 0x56),   // 67
    (0x52, 0x53),   // 68
    (0x58, 0x59),   // 69
    (0x50, 0x51),   // 70
];

/// Inline of WA's hard-coded `NONE` resolution: reads
/// `LocalizationData.offsets[0x11]` from the secondary then primary
/// records, falling back to the literal `"NONE"` pointer at
/// [`va::PTR_S_NONE`] when neither overrides id 0x11.
unsafe fn resolve_none_label() -> *const c_char {
    unsafe {
        for slot in [
            va::G_LOCALIZATION_DATA_SECONDARY,
            va::G_LOCALIZATION_DATA_PRIMARY,
        ] {
            let data = *(rb(slot) as *const *const LocalizationData);
            if data.is_null() {
                continue;
            }
            let off = *(*data).offsets.add(NONE_OFFSET_INDEX);
            if off != 0 {
                return (*data).base.wrapping_offset(off as isize);
            }
        }
        *(rb(va::PTR_S_NONE) as *const *const c_char)
    }
}

/// Pure Rust port of WA's `InitWeaponNameStrings` (0x0053C130,
/// `__usercall(ESI = weapon_table, EDI = localized_template)`).
pub unsafe fn init_weapon_name_strings(world: *mut GameWorld) {
    unsafe {
        let table = (*world).weapon_table;
        let loc_ctx = (*world).localized_template;

        let placeholder = wa_load_string(PLACEHOLDER_NAME);
        for entry in (*table).entries.iter_mut() {
            entry.name1 = placeholder;
            entry.name2 = placeholder;
        }

        let none = resolve_none_label();
        (*table).entries[0].name1 = none;
        (*table).entries[0].name2 = none;

        for (i, &(name2_token, name1_token)) in WEAPON_NAME_TOKENS.iter().enumerate() {
            let entry = &mut (*table).entries[i + 1];
            entry.name2 = resolve_raw(loc_ctx, name2_token);
            entry.name1 = resolve_raw(loc_ctx, name1_token);
        }
    }
}
