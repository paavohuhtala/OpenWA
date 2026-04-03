//! DDGame__LoadFonts (0x570F30) and DDGameWrapper__LoadFontExtension (0x570E80).
//!
//! Loads 28 bitmap fonts (.fnt) and 23 font extensions (.fex) into the display,
//! then sets the font palette. Called from DDGame__Constructor during loading.

use core::ffi::c_char;
use heapless::CString;

use crate::display::dd_display::DDDisplay;
use crate::engine::ddgame_wrapper::DDGameWrapper;

/// Total number of font slots loaded (1-based, so slots 1..=28).
const FONT_COUNT: u32 = 0x1C;

/// Number of font slots that get font extensions loaded (1..=23).
const FONT_EXT_COUNT: u32 = 0x17;

/// Font filenames (.fnt), indexed 1..=28 (slot 0 unused).
const FONT_FILENAMES: [&[u8]; 29] = [
    b"\0",             // [0] unused
    b"smlred1.fnt\0",  // [1]
    b"smlblu1.fnt\0",  // [2]
    b"smlgrn1.fnt\0",  // [3]
    b"smlyel1.fnt\0",  // [4]
    b"smlppl1.fnt\0",  // [5]
    b"smlcyn1.fnt\0",  // [6]
    b"smlwht1.fnt\0",  // [7]
    b"smlwht2.fnt\0",  // [8]
    b"stdred1.fnt\0",  // [9]
    b"stdblu1.fnt\0",  // [10]
    b"stdgrn1.fnt\0",  // [11]
    b"stdyel1.fnt\0",  // [12]
    b"stdppl1.fnt\0",  // [13]
    b"stdcyn1.fnt\0",  // [14]
    b"stdwht1.fnt\0",  // [15]
    b"stdwht2.fnt\0",  // [16]
    b"medred1.fnt\0",  // [17]
    b"medblu1.fnt\0",  // [18]
    b"medgrn1.fnt\0",  // [19]
    b"medyel1.fnt\0",  // [20]
    b"medppl1.fnt\0",  // [21]
    b"medcyn1.fnt\0",  // [22]
    b"medwht1.fnt\0",  // [23]
    b"lnumred1.fnt\0", // [24]
    b"lnumwht1.fnt\0", // [25]
    b"lnumred2.fnt\0", // [26]
    b"lnumwht2.fnt\0", // [27]
    b"digiwht.fnt\0",  // [28]
];

/// Font ID → size category index.
/// 0 = sml, 1 = std, 2 = med, 3 = bignum, 4 = digi.
/// Original data at 0x6A90E8.
const FONT_SIZE_CATEGORY: [u32; 29] = [
    5, // [0] unused
    0, 0, 0, 0, 0, 0, 0, 0, // [1-8]   sml
    1, 1, 1, 1, 1, 1, 1, 1, // [9-16]  std
    2, 2, 2, 2, 2, 2, 2, // [17-23] med
    3, 3, 3, 3, // [24-27] bignum
    4, // [28]    digi
];

/// Font ID → color index. Original data at 0x6A9160.
const FONT_COLOR_INDEX: [u32; 29] = [
    10, // [0]
    0, 1, 2, 3, 4, 5, 6, 7, // [1-8]   red, blu, grn, yel, ppl, cyn, wht1, wht2
    0, 1, 2, 3, 4, 5, 6, 7, // [9-16]
    0, 1, 2, 3, 4, 5, 6, // [17-23]
    9, 6, 8, 7, // [24-27]
    6, // [28]
];

/// Color index → gfx_color_table index (0-based).
/// Original data at 0x6A91D4 (1-based offsets from DDGame+0x7308, converted to 0-based).
const COLOR_TABLE_INDEX: [usize; 10] = [0, 1, 2, 3, 4, 5, 8, 6, 9, 10];

/// Size category → .fex path prefix.
const SIZE_PREFIXES: [&str; 5] = ["sml", "std", "med", "bignum", ""];

/// Extended character map (61 bytes). Original data at 0x66ACD8.
/// Defines which character codes are present in .fex font extension files.
const FONT_EXT_CHAR_MAP: [u8; 61] = [
    0xBF, 0xA1, 0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x8B, 0x8C, 0x8D,
    0x8E, 0x8F, 0x90, 0x91, 0x92, 0x93, 0x94, 0x96, 0x97, 0x98, 0x99, 0x9A, 0x9B, 0x9C, 0x9D, 0x9E,
    0xA2, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xAB, 0xAC, 0xAD, 0xAE, 0xAF, 0xB0, 0xB1, 0xB2, 0xB3,
    0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0x0A, 0xB9, 0xBA, 0xBB, 0xBC, 0xBD, 0xBE, 0x00,
];

/// Load a font extension (.fex) for the given font slot.
///
/// Port of DDGameWrapper__LoadFontExtension (0x570E80, stdcall, RET 0x8).
/// Formats the .fex path from the font's size category, copies the character map,
/// resolves the palette value, and calls DDDisplay::load_font_extension.
unsafe fn load_font_extension(wrapper: *mut DDGameWrapper, font_id: u32) {
    let size_category = FONT_SIZE_CATEGORY[font_id as usize];
    let prefix = SIZE_PREFIXES[size_category as usize];

    // Format path: "Data\Gfx\FontExt\{prefix}2.fex"
    let mut path: CString<64> = CString::new();
    let _ = path.extend_from_bytes(b"Data\\Gfx\\FontExt\\");
    let _ = path.extend_from_bytes(prefix.as_bytes());
    let _ = path.extend_from_bytes(b"2.fex");

    // Copy character map to stack (original does REP MOVSD + MOVSB)
    let char_map = FONT_EXT_CHAR_MAP;

    // Resolve palette value: font_id → color_index → gfx_color_table entry
    let color_index = FONT_COLOR_INDEX[font_id as usize];
    let table_index = COLOR_TABLE_INDEX[color_index as usize];
    let ddgame = (*wrapper).ddgame;
    let palette_value = (*ddgame).gfx_color_table[table_index];

    let display = (*wrapper).display;
    DDDisplay::load_font_extension_raw(
        display,
        font_id as i32,
        path.as_ptr() as *const c_char,
        char_map.as_ptr(),
        palette_value,
        0,
    );
}

/// Load all bitmap fonts and font extensions.
///
/// Port of DDGame__LoadFonts (0x570F30, usercall ESI=DDGameWrapper).
///
/// Phase 1: Loads 28 .fnt bitmap fonts via DDDisplay::load_font.
/// Phase 2: Loads .fex font extensions for font slots 1-23.
/// Phase 3: Sets the font palette via DDDisplay::set_font_palette.
pub unsafe fn load_fonts(wrapper: *mut DDGameWrapper) {
    let display = (*wrapper).display;
    let gfx_dir = (*wrapper).primary_gfx_dir;

    // Phase 1: Load all 28 .fnt font files
    for font_id in 1..=FONT_COUNT {
        let filename = FONT_FILENAMES[font_id as usize];
        DDDisplay::load_font_raw(
            display,
            1,
            font_id as i32,
            gfx_dir,
            filename.as_ptr() as *const c_char,
        );
    }

    // Phase 2: Load font extensions for slots 1-23
    for font_id in 1..=FONT_EXT_COUNT {
        load_font_extension(wrapper, font_id);
    }

    // Phase 3: Set font palette — passes font count and gfx_color_table[7].
    let ddgame = (*wrapper).ddgame;
    let palette_value = (*ddgame).gfx_color_table[7];
    DDDisplay::set_font_palette_raw(display, FONT_COUNT, palette_value);
}
