//! Portable decoder for WA's `.pal` standalone palette files.
//!
//! WA ships palettes in the standard Microsoft RIFF PAL format
//! (`RIFF` / `PAL ` / `data`). Every shipped `.pal` file inspected so far is
//! 1168 bytes: a 12-byte RIFF header, a 4-byte `PAL ` form id, an 8-byte
//! `data` chunk header, then a 4-byte palette header (version 0x0300, entry
//! count 0x0100) followed by 256 × 4-byte entries. Three trailing
//! 32-byte-of-zero sub-chunks (`offl`, `tran`, `unde`) follow the entries;
//! their purpose is unknown and the game accepts files with them stripped,
//! so we skip them.
//!
//! **Note on "mostly black" palettes**: WA palettes are sparse — a single
//! `.pal` file populates only the color slots it owns (e.g. `water.pal`
//! fills indices 120-126 with shades of blue). The remaining slots are zero
//! on disk and get populated from other palette files at runtime. A viewer
//! rendering only 7 non-zero swatches out of 256 is expected, not a parser
//! bug.
//!
//! Per-byte layout and the `offl`/`tran`/`unde` chunk sizes cross-referenced
//! against the Worms Knowledge Base PAL format writeup.
//!
//! | Offset | Size   | Field                                          |
//! |--------|--------|------------------------------------------------|
//! | 0x00   | 4      | `"RIFF"`                                       |
//! | 0x04   | 4      | RIFF payload size (le u32)                     |
//! | 0x08   | 4      | `"PAL "` — RIFF form id                        |
//! | 0x0C   | 4      | `"data"` — chunk id                            |
//! | 0x10   | 4      | data chunk size (le u32)                       |
//! | 0x14   | 2      | version (le u16, typically `0x0300`)           |
//! | 0x16   | 2      | entry_count (le u16, typically `0x0100`)       |
//! | 0x18   | n × 4  | entries: `[r, g, b, flags]`                    |
//!
//! The fourth byte per entry is a flags/reserved field; Microsoft's
//! `LOGPALETTE` calls it `peFlags`. WA's shipped palettes store it as `0`.

/// RIFF container magic: `"RIFF"` as little-endian `u32`.
pub const RIFF_MAGIC: u32 = u32::from_le_bytes(*b"RIFF");
/// RIFF form id for palette files: `"PAL "`.
pub const PAL_FORM_ID: u32 = u32::from_le_bytes(*b"PAL ");
/// RIFF chunk id for the palette data: `"data"`.
pub const PAL_DATA_CHUNK: u32 = u32::from_le_bytes(*b"data");

/// Size of the fixed RIFF/PAL prefix up to (but not including) the `data`
/// chunk's size field: `RIFF` + riff_size + `PAL ` + `data`.
const HEADER_PREFIX: usize = 16;
/// Offset of the first palette entry in a standard RIFF PAL file.
const ENTRIES_OFFSET: usize = 0x18;

/// One palette entry. `flags` is a reserved/peFlags byte, typically `0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PalEntry {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub flags: u8,
}

/// A decoded RIFF PAL palette.
#[derive(Debug, Clone)]
pub struct DecodedPal {
    /// Palette version word (WA palettes use `0x0300`).
    pub version: u16,
    pub entries: Vec<PalEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PalDecodeError {
    /// File is smaller than the fixed RIFF/PAL header.
    TooShort,
    /// First four bytes are not `"RIFF"`.
    BadMagic,
    /// Form id at offset 8 is not `"PAL "`.
    BadFormId,
    /// Chunk id at offset 12 is not `"data"`.
    BadDataChunk,
    /// The `data` chunk declares a size that doesn't fit the file or
    /// doesn't match the declared `entry_count`.
    BadDataSize,
    /// The file ends before all `entry_count` entries have been read.
    Truncated,
}

/// Parse a standalone `.pal` file.
///
/// Trailing sub-chunks after the palette entries (`offl`, `tran`, `unde`)
/// are ignored.
pub fn pal_decode(raw: &[u8]) -> Result<DecodedPal, PalDecodeError> {
    use byteorder::{LittleEndian, ReadBytesExt};
    use std::io::{Cursor, Read};

    if raw.len() < ENTRIES_OFFSET {
        return Err(PalDecodeError::TooShort);
    }

    let mut cur = Cursor::new(raw);

    let mut read_tag = |cur: &mut Cursor<&[u8]>| -> Result<u32, PalDecodeError> {
        let mut tag = [0u8; 4];
        cur.read_exact(&mut tag)
            .map_err(|_| PalDecodeError::TooShort)?;
        Ok(u32::from_le_bytes(tag))
    };

    if read_tag(&mut cur)? != RIFF_MAGIC {
        return Err(PalDecodeError::BadMagic);
    }
    // Outer RIFF payload length — ignored; we validate sizes inline below.
    let _riff_size = cur
        .read_u32::<LittleEndian>()
        .map_err(|_| PalDecodeError::TooShort)?;
    if read_tag(&mut cur)? != PAL_FORM_ID {
        return Err(PalDecodeError::BadFormId);
    }
    if read_tag(&mut cur)? != PAL_DATA_CHUNK {
        return Err(PalDecodeError::BadDataChunk);
    }

    let data_size = cur
        .read_u32::<LittleEndian>()
        .map_err(|_| PalDecodeError::TooShort)? as usize;
    let version = cur
        .read_u16::<LittleEndian>()
        .map_err(|_| PalDecodeError::TooShort)?;
    let entry_count = cur
        .read_u16::<LittleEndian>()
        .map_err(|_| PalDecodeError::TooShort)? as usize;

    // The `data` chunk payload is `version (2) + entry_count (2) + entries (n*4)`.
    let expected = 4 + entry_count * 4;
    if data_size < expected {
        return Err(PalDecodeError::BadDataSize);
    }
    if HEADER_PREFIX + 4 + data_size > raw.len() {
        return Err(PalDecodeError::BadDataSize);
    }

    let mut entries = Vec::with_capacity(entry_count);
    for _ in 0..entry_count {
        let mut rgba = [0u8; 4];
        cur.read_exact(&mut rgba)
            .map_err(|_| PalDecodeError::Truncated)?;
        entries.push(PalEntry {
            r: rgba[0],
            g: rgba[1],
            b: rgba[2],
            flags: rgba[3],
        });
    }

    Ok(DecodedPal { version, entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    const WATER_PAL: &[u8] = include_bytes!("../../../testdata/assets/water.pal");

    #[test]
    fn decodes_water_pal() {
        let pal = pal_decode(WATER_PAL).expect("water.pal must decode");
        assert_eq!(pal.version, 0x0300);
        assert_eq!(pal.entries.len(), 256);
        // At least one entry must carry non-zero color data — a palette
        // file of pure zeros would be suspicious.
        assert!(pal.entries.iter().any(|e| e.r | e.g | e.b != 0));
    }

    #[test]
    fn rejects_short_input() {
        assert!(matches!(pal_decode(&[]), Err(PalDecodeError::TooShort)));
        assert!(matches!(
            pal_decode(&[0; 16]),
            Err(PalDecodeError::TooShort)
        ));
    }

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = WATER_PAL.to_vec();
        bytes[0] = b'X';
        assert!(matches!(pal_decode(&bytes), Err(PalDecodeError::BadMagic)));
    }

    #[test]
    fn rejects_bad_form_id() {
        let mut bytes = WATER_PAL.to_vec();
        bytes[8] = b'X';
        assert!(matches!(pal_decode(&bytes), Err(PalDecodeError::BadFormId)));
    }
}
