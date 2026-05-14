//! Minimal Windows-1252 (CP1252) ↔ UTF-8 codec.
//!
//! Worms Armageddon stores every user-facing string (team names, worm
//! names, soundbank labels, flag filenames, replay HUD text, …) in
//! Windows-1252. Rust's `str` is UTF-8, so any time we surface those
//! bytes to a UI or write user text back into WA memory we need to
//! convert. This module is the one and only place that conversion lives.
//!
//! Single-byte encoding: each input byte maps to exactly one Unicode
//! code point (and vice versa). The only quirky range is `0x80..=0x9F`,
//! where five bytes (`0x81`, `0x8D`, `0x8F`, `0x90`, `0x9D`) are
//! officially "undefined" — we round-trip them through `U+0081` /
//! `U+008D` / etc. so byte-exact round-trips work.

/// Code-point each byte 0x00..=0xFF decodes to, per Windows-1252.
const TABLE: [char; 256] = build_table();

const fn build_table() -> [char; 256] {
    let mut t = ['\0'; 256];
    let mut i = 0;
    while i < 256 {
        // Identity for 0x00..=0x7F (ASCII) and 0xA0..=0xFF (Latin-1).
        if i < 0x80 || i >= 0xA0 {
            // SAFETY: every value 0..=0x7F and 0xA0..=0xFF is a valid scalar.
            t[i] = unsafe { char::from_u32_unchecked(i as u32) };
        }
        i += 1;
    }
    // Windows-1252 punctuation/control block 0x80..=0x9F.
    t[0x80] = '\u{20AC}'; // €
    t[0x81] = '\u{0081}'; // (undefined)
    t[0x82] = '\u{201A}'; // ‚
    t[0x83] = '\u{0192}'; // ƒ
    t[0x84] = '\u{201E}'; // „
    t[0x85] = '\u{2026}'; // …
    t[0x86] = '\u{2020}'; // †
    t[0x87] = '\u{2021}'; // ‡
    t[0x88] = '\u{02C6}'; // ˆ
    t[0x89] = '\u{2030}'; // ‰
    t[0x8A] = '\u{0160}'; // Š
    t[0x8B] = '\u{2039}'; // ‹
    t[0x8C] = '\u{0152}'; // Œ
    t[0x8D] = '\u{008D}'; // (undefined)
    t[0x8E] = '\u{017D}'; // Ž
    t[0x8F] = '\u{008F}'; // (undefined)
    t[0x90] = '\u{0090}'; // (undefined)
    t[0x91] = '\u{2018}'; // '
    t[0x92] = '\u{2019}'; // '
    t[0x93] = '\u{201C}'; // "
    t[0x94] = '\u{201D}'; // "
    t[0x95] = '\u{2022}'; // •
    t[0x96] = '\u{2013}'; // –
    t[0x97] = '\u{2014}'; // —
    t[0x98] = '\u{02DC}'; // ˜
    t[0x99] = '\u{2122}'; // ™
    t[0x9A] = '\u{0161}'; // š
    t[0x9B] = '\u{203A}'; // ›
    t[0x9C] = '\u{0153}'; // œ
    t[0x9D] = '\u{009D}'; // (undefined)
    t[0x9E] = '\u{017E}'; // ž
    t[0x9F] = '\u{0178}'; // Ÿ
    t
}

/// Sentinel used when a Unicode scalar has no CP1252 representation.
/// Mirrors WA's own behaviour when a UI text field receives a paste with
/// characters outside its codepage.
pub const REPLACEMENT_BYTE: u8 = b'?';

/// Decode raw Windows-1252 bytes to a UTF-8 `String`.
pub fn decode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len());
    for &b in bytes {
        s.push(TABLE[b as usize]);
    }
    s
}

/// Decode the leading NUL-terminated portion of a fixed-width CP1252
/// field to `String`. Bytes past the first NUL are dropped.
pub fn decode_cstr(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    decode(&bytes[..end])
}

/// Encode UTF-8 → Windows-1252, replacing unrepresentable scalars with
/// [`REPLACEMENT_BYTE`].
pub fn encode_lossy(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| char_to_cp1252(c).unwrap_or(REPLACEMENT_BYTE))
        .collect()
}

/// Look up the single-byte CP1252 encoding of a Unicode scalar.
fn char_to_cp1252(c: char) -> Option<u8> {
    let cp = c as u32;
    // ASCII + Latin-1 supplement: direct identity, minus the
    // CP1252-specific block 0x80..=0x9F which Latin-1 leaves undefined.
    if cp < 0x80 || (0xA0..=0xFF).contains(&cp) {
        return Some(cp as u8);
    }
    // Punctuation / symbol overrides in 0x80..=0x9F.
    Some(match cp {
        0x20AC => 0x80,
        0x0081 => 0x81,
        0x201A => 0x82,
        0x0192 => 0x83,
        0x201E => 0x84,
        0x2026 => 0x85,
        0x2020 => 0x86,
        0x2021 => 0x87,
        0x02C6 => 0x88,
        0x2030 => 0x89,
        0x0160 => 0x8A,
        0x2039 => 0x8B,
        0x0152 => 0x8C,
        0x008D => 0x8D,
        0x017D => 0x8E,
        0x008F => 0x8F,
        0x0090 => 0x90,
        0x2018 => 0x91,
        0x2019 => 0x92,
        0x201C => 0x93,
        0x201D => 0x94,
        0x2022 => 0x95,
        0x2013 => 0x96,
        0x2014 => 0x97,
        0x02DC => 0x98,
        0x2122 => 0x99,
        0x0161 => 0x9A,
        0x203A => 0x9B,
        0x0153 => 0x9C,
        0x009D => 0x9D,
        0x017E => 0x9E,
        0x0178 => 0x9F,
        _ => return None,
    })
}

/// Encode UTF-8 into a fixed-width CP1252 buffer (`dst`). The result is
/// null-terminated within `dst.len()` bytes: the encoded body is
/// truncated to `dst.len() - 1` if necessary, and all remaining bytes
/// are set to `0`. Returns the number of bytes written before the NUL.
///
/// This is the canonical way to stamp user text into WA's fixed-width
/// name fields (team_record.name, worm name slots, soundbank-name
/// fields, etc.) without overrunning the buffer or leaving stale bytes.
pub fn encode_into_fixed(dst: &mut [u8], src: &str) -> usize {
    let cap = dst.len().saturating_sub(1);
    let encoded = encode_lossy(src);
    let copy = encoded.len().min(cap);
    dst[..copy].copy_from_slice(&encoded[..copy]);
    for slot in &mut dst[copy..] {
        *slot = 0;
    }
    copy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_roundtrips() {
        let s = "Hello, world!";
        assert_eq!(decode(s.as_bytes()), s);
        assert_eq!(encode_lossy(s), s.as_bytes());
    }

    #[test]
    fn latin1_diacritics_roundtrip() {
        // The test team that triggered this fix.
        let name = "Åäö";
        let encoded = encode_lossy(name);
        assert_eq!(encoded, [0xC5, 0xE4, 0xF6]);
        assert_eq!(decode(&encoded), name);
    }

    #[test]
    fn worm_names_with_diacritics() {
        for name in ["Äéó", "Ëééíí", "Ññü", "Whelk", "Snake Bitey"] {
            let bytes = encode_lossy(name);
            assert_eq!(decode(&bytes), name);
        }
    }

    #[test]
    fn cp1252_specific_block() {
        // € sits at 0x80 in CP1252 but is a 3-byte UTF-8 char.
        assert_eq!(encode_lossy("€"), vec![0x80]);
        assert_eq!(decode(&[0x80]), "€");
    }

    #[test]
    fn unrepresentable_replaced() {
        // Japanese hiragana not in CP1252 → '?'.
        let s = "Wormえ";
        let bytes = encode_lossy(s);
        assert_eq!(bytes, b"Worm?");
    }

    #[test]
    fn decode_cstr_stops_at_nul() {
        let buf = b"team\0stale leftover";
        assert_eq!(decode_cstr(buf), "team");
    }

    #[test]
    fn encode_into_fixed_truncates_and_nuls() {
        let mut dst = [0xAAu8; 8];
        let n = encode_into_fixed(&mut dst, "Åäö");
        // "Åäö" is 3 CP1252 bytes; fits with 5 trailing NULs.
        assert_eq!(n, 3);
        assert_eq!(dst, [0xC5, 0xE4, 0xF6, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn encode_into_fixed_reserves_terminator() {
        let mut dst = [0u8; 4];
        let n = encode_into_fixed(&mut dst, "ABCDEF");
        // Cap = 3 bytes of body + 1 NUL.
        assert_eq!(n, 3);
        assert_eq!(&dst, b"ABC\0");
    }
}
