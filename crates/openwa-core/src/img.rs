//! Portable decoder for WA's `.img` image format.
//!
//! WA uses a custom "IMG" format for static graphics (gradients, masks,
//! HUD elements). Two on-disk variants exist, distinguished by whether
//! the blob carries its own `IMG\x1A` magic header:
//!
//! - **Tagged** ([`img_decode`]): starts with the `"IMG\x1A"` magic and a
//!   flags word that selects between 1bpp or 8bpp, LZSS or raw pixels,
//!   palette or no palette, and extended-header encoding. This is the
//!   full `.img` format served out of `.dir` archives and parsed by
//!   `IMG_Decode` (0x4F5F80).
//! - **Headerless** ([`img_decode_headerless`]): no magic, no flags, no
//!   compression, no variant selection — always 8bpp + palette + raw
//!   pixels, with a fixed layout (10-byte prefix, palette count, RGB
//!   triplets, width, height, 4-byte-aligned pixel block). Used by
//!   `DisplayGfx__Constructor` (0x4F5E80) for in-memory cache entries
//!   that don't need to re-identify themselves.
//!
//! Both take a byte slice and a palette-mapping callback, and return a
//! [`DecodedImg`] with a fresh Rust-owned pixel buffer. The WA-facing
//! wrappers in `openwa-game` then copy those pixels into a `BitGrid` and
//! update WA's global palette byte counter.
//!
//! ## Tagged format
//!
//! | Offset | Size | Field                                   |
//! |--------|------|-----------------------------------------|
//! | 0      | 4    | Magic `"IMG\x1A"` (`0x1A474D49`)        |
//! | 4      | 4    | `data_size` — total bytes in IMG payload |
//! | 8      | 2    | `flags`                                  |
//!
//! Flags layout: low byte is `bpp` (1 or 8). Bit 0x4000 = LZSS-compressed
//! pixels. Bit 0x8000 = palette present. Bits 0x3FF6 = extended header
//! (rare) — in that case `flags` is actually the first byte of a null-
//! terminated string and real flags follow.
//!
//! If `palette` bit set: a `u16` palette_count then `palette_count * 3`
//! RGB triplets. Each triplet is passed to `map_color` to produce a
//! runtime palette index; local pixel value `i` remaps to `lut[i]` where
//! `lut[0] = 0` (transparent) and `lut[1 + k] = map_color(triplet_k)`.
//! See `feedback_layer_sprite_palette.md` for the off-by-one rationale.
//!
//! Then `u16 width`, `u16 height`. If the caller set the `align` flag,
//! the stream is advanced to the next 4-byte boundary (relative to the
//! start of the IMG data — position `0` at magic).
//!
//! Then either LZSS-compressed pixel data (decompressed via
//! [`sprite_lzss_decode_slice`](crate::sprite_lzss::sprite_lzss_decode_slice),
//! which applies `lut` inline) or raw row-by-row pixel data
//! (`ceil(width * bpp / 8)` bytes per row, packed tight; we copy into a
//! 4-byte-row-aligned buffer matching `BitGrid::init`'s stride, then
//! remap 8bpp bytes through `lut` if a palette was present).
//!
//! ## Headerless format
//!
//! Skip 10 bytes, then `u16 palette_count`, `palette_count * 3` RGB
//! triplets, `u16 width`, `u16 height`. Pixel data starts at the next
//! 4-byte-aligned offset within the raw buffer and is tightly packed
//! (`row_stride = width`). Always 8bpp. The palette remap runs on
//! `(width / 4) * 4` bytes per row — trailing `width % 4` pixels are
//! left as-is (matches the original's DWORD-granularity loop).

use crate::lzss_decode::lzss_decode_slice;

/// IMG file magic: `"IMG\x1A"` as little-endian `u32`.
pub const IMG_MAGIC: u32 = 0x1A474D49;

const FLAG_HAS_PALETTE: u16 = 0x8000;
const FLAG_LZSS_COMPRESSED: u16 = 0x4000;
const FLAG_EXTENDED_HEADER: u16 = 0x3FF6;

/// Decoded IMG image with Rust-owned pixel data.
#[derive(Debug, Clone)]
pub struct DecodedImg {
    /// Bits per pixel. `1` for collision bitmaps, `8` for display images.
    pub bpp: u8,
    pub width: u32,
    pub height: u32,
    /// Bytes per row. Matches `BitGrid::init`'s 4-byte-aligned formula
    /// for the streaming path, and equals `width` for the cached 8bpp
    /// path.
    pub row_stride: u32,
    /// Row-major pixel buffer of length `row_stride * height`.
    pub pixels: Vec<u8>,
    /// `palette_count * 3` — the amount the caller should add to WA's
    /// global `G_SPRITE_PALETTE_BYTES` counter. Zero if no palette.
    pub palette_rgb_bytes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImgDecodeError {
    /// First four bytes are not `IMG\x1A`.
    BadMagic,
    /// `bpp` (low byte of `flags`) is not `1` or `8`.
    UnsupportedBpp(u8),
    /// Ran out of input bytes before the header / pixel data was complete.
    UnexpectedEof,
    /// Extended-header string never found its null terminator.
    MalformedExtendedHeader,
}

// ─── Internal cursor ────────────────────────────────────────────────────────

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn pos(&self) -> usize {
        self.pos
    }

    fn seek(&mut self, pos: usize) -> Result<(), ImgDecodeError> {
        if pos > self.data.len() {
            return Err(ImgDecodeError::UnexpectedEof);
        }
        self.pos = pos;
        Ok(())
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], ImgDecodeError> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(ImgDecodeError::UnexpectedEof)?;
        if end > self.data.len() {
            return Err(ImgDecodeError::UnexpectedEof);
        }
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn read_u8(&mut self) -> Result<u8, ImgDecodeError> {
        Ok(self.take(1)?[0])
    }

    fn read_u16_le(&mut self) -> Result<u16, ImgDecodeError> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }

    fn read_u32_le(&mut self) -> Result<u32, ImgDecodeError> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
}

// ─── Tagged format (IMG\x1A magic + flags-driven variants) ─────────────────

/// Portable port of `IMG_Decode` (0x4F5F80). Decodes the **tagged** IMG
/// format — the one that starts with the `IMG\x1A` magic and carries a
/// flags word describing bpp / compression / palette / extended header.
///
/// `data` is the full IMG payload starting at the magic bytes. The caller
/// should size it to the IMG's `data_size` field (readable as the `u32`
/// at offset 4). `align`, if true, advances past 0–3 padding bytes after
/// the width/height header so the pixel data starts on a 4-byte boundary
/// relative to the start of `data`.
///
/// `map_color` receives each palette RGB triplet (packed as the low 24
/// bits of `u32`, `r | g << 8 | b << 16`) and returns the runtime palette
/// index to map that color to.
pub fn img_decode(
    data: &[u8],
    align: bool,
    mut map_color: impl FnMut(u32) -> u8,
) -> Result<DecodedImg, ImgDecodeError> {
    let mut cur = Cursor::new(data);

    let magic = cur.read_u32_le()?;
    if magic != IMG_MAGIC {
        return Err(ImgDecodeError::BadMagic);
    }

    let data_size = cur.read_u32_le()?;
    let mut flags = cur.read_u16_le()?;

    // Extended header: the bytes we just read as `flags` are actually the
    // start of a null-terminated string. Seek back, skip to the null,
    // then re-read the real flags.
    if (flags & FLAG_EXTENDED_HEADER) != 0 {
        cur.seek(cur.pos() - 2)?;
        loop {
            let b = cur
                .read_u8()
                .map_err(|_| ImgDecodeError::MalformedExtendedHeader)?;
            if b == 0 {
                break;
            }
        }
        flags = cur.read_u16_le()?;
    }

    // Palette: 0 stays transparent, subsequent entries are mapped RGB.
    let mut lut = [0u8; 256];
    let mut palette_rgb_bytes = 0u32;
    if (flags & FLAG_HAS_PALETTE) != 0 {
        let palette_count = cur.read_u16_le()? as usize;
        let rgb_bytes = palette_count * 3;
        let rgb = cur.take(rgb_bytes)?;
        palette_rgb_bytes = rgb_bytes as u32;
        for i in 0..palette_count {
            let r = rgb[i * 3] as u32;
            let g = rgb[i * 3 + 1] as u32;
            let b = rgb[i * 3 + 2] as u32;
            lut[i + 1] = map_color(r | (g << 8) | (b << 16));
        }
    }

    let width = cur.read_u16_le()? as u32;
    let height = cur.read_u16_le()? as u32;

    let bpp = (flags & 0xFF) as u8;
    if bpp != 1 && bpp != 8 {
        return Err(ImgDecodeError::UnsupportedBpp(bpp));
    }

    // Matches BitGrid::init (0x4F6370): bits = bpp*width + 7,
    // row_stride = ((bits >> 3) + 3) & !3.
    let bits = (bpp as u32).wrapping_mul(width).wrapping_add(7);
    let row_stride = ((bits >> 3) + 3) & !3u32;

    if align {
        while (cur.pos() & 3) != 0 {
            let _ = cur.read_u8()?;
        }
    }

    let total_pixels = (row_stride as usize) * (height as usize);
    let mut pixels = vec![0u8; total_pixels];

    if (flags & FLAG_LZSS_COMPRESSED) != 0 {
        // LZSS: consume all remaining bytes up to data_size. The decoder
        // applies `lut` inline, so no separate remap pass.
        let remaining = (data_size as usize).saturating_sub(cur.pos());
        let src = cur.take(remaining)?;
        lzss_decode_slice(&mut pixels, src, &lut);
    } else {
        // Raw rows. Packed tight on disk, copied into stride-aligned buffer.
        let bytes_per_row = (width * bpp as u32).div_ceil(8);
        for row in 0..height {
            let src = cur.take(bytes_per_row as usize)?;
            let dst_start = (row * row_stride) as usize;
            pixels[dst_start..dst_start + bytes_per_row as usize].copy_from_slice(src);
        }

        // For 8bpp with a palette, remap the entire row_stride-wide span
        // through lut (matches `remap_pixels_through_lut` with
        // width_dwords = row_stride / 4).
        if (flags & FLAG_HAS_PALETTE) != 0 && bpp == 8 {
            for row in 0..height {
                let start = (row * row_stride) as usize;
                for i in 0..row_stride as usize {
                    pixels[start + i] = lut[pixels[start + i] as usize];
                }
            }
        }
    }

    Ok(DecodedImg {
        bpp,
        width,
        height,
        row_stride,
        pixels,
        palette_rgb_bytes,
    })
}

// ─── Headerless format (no magic, fixed 8bpp + palette) ────────────────────

/// Portable port of `DisplayGfx__Constructor` (0x4F5E80). Decodes the
/// **headerless** IMG format — no magic, no flags, always 8bpp + palette
/// + raw pixels, with a fixed layout.
///
/// `raw` is the full blob. The first 10 bytes are skipped, then
/// palette_count / RGB triplets / width / height are read, pixel data
/// starts at the next 4-byte-aligned offset. `row_stride == width`
/// (tight-packed).
pub fn img_decode_headerless(
    raw: &[u8],
    mut map_color: impl FnMut(u32) -> u8,
) -> Result<DecodedImg, ImgDecodeError> {
    let mut cur = Cursor::new(raw);
    cur.seek(0x0A)?;

    let palette_count = cur.read_u16_le()? as usize;

    let mut lut = [0u8; 256];
    let rgb_bytes = palette_count * 3;
    let rgb = cur.take(rgb_bytes)?;
    for i in 0..palette_count {
        let r = rgb[i * 3] as u32;
        let g = rgb[i * 3 + 1] as u32;
        let b = rgb[i * 3 + 2] as u32;
        lut[i + 1] = map_color(r | (g << 8) | (b << 16));
    }

    let width = cur.read_u16_le()? as u32;
    let height = cur.read_u16_le()? as u32;

    // Pixel offset = ceil-align-4 of header end. Header ends at
    // `cur.pos() - 2` (we advanced past width but original code only
    // consumes +2 past width, leaving height "unaccounted for" in its
    // +5 bump — see source).
    let wh_start = 0x0C + palette_count * 3;
    let aligned_offset = (wh_start + 7) & !3usize;

    let row_stride = width;
    let pixel_bytes = (row_stride as usize) * (height as usize);
    let end = aligned_offset
        .checked_add(pixel_bytes)
        .ok_or(ImgDecodeError::UnexpectedEof)?;
    if end > raw.len() {
        return Err(ImgDecodeError::UnexpectedEof);
    }

    let mut pixels = raw[aligned_offset..end].to_vec();

    // Remap through LUT on DWORD-aligned span per row — trailing
    // (width % 4) pixels are left as-is, matching the original.
    let remap_width = (width / 4) * 4;
    for row in 0..height {
        let start = (row * row_stride) as usize;
        for i in 0..remap_width as usize {
            pixels[start + i] = lut[pixels[start + i] as usize];
        }
    }

    Ok(DecodedImg {
        bpp: 8,
        width,
        height,
        row_stride,
        pixels,
        palette_rgb_bytes: rgb_bytes as u32,
    })
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn write_u16(v: &mut Vec<u8>, x: u16) {
        v.extend_from_slice(&x.to_le_bytes());
    }
    fn write_u32(v: &mut Vec<u8>, x: u32) {
        v.extend_from_slice(&x.to_le_bytes());
    }

    #[test]
    fn bad_magic() {
        let data = [0u8; 16];
        let err = img_decode(&data, false, |_| 0).unwrap_err();
        assert_eq!(err, ImgDecodeError::BadMagic);
    }

    #[test]
    fn raw_8bpp_no_palette() {
        // 4x2 8bpp, no palette, no LZSS, no align, no extended header.
        // flags = 0x0008 (bpp=8)
        let width: u16 = 4;
        let height: u16 = 2;
        let mut buf = Vec::new();
        write_u32(&mut buf, IMG_MAGIC);
        // data_size placeholder; fill after
        let ds_off = buf.len();
        write_u32(&mut buf, 0);
        write_u16(&mut buf, 0x0008); // flags: bpp=8
        write_u16(&mut buf, width);
        write_u16(&mut buf, height);
        // 4 bytes per row * 2 rows
        buf.extend_from_slice(&[1, 2, 3, 4]);
        buf.extend_from_slice(&[5, 6, 7, 8]);
        // patch data_size
        let ds = buf.len() as u32;
        buf[ds_off..ds_off + 4].copy_from_slice(&ds.to_le_bytes());

        let d = img_decode(&buf, false, |_| panic!("no palette")).unwrap();
        assert_eq!(d.bpp, 8);
        assert_eq!(d.width, 4);
        assert_eq!(d.height, 2);
        assert_eq!(d.row_stride, 4); // (4*8+7)/8 = 4, align 4 -> 4
        assert_eq!(d.palette_rgb_bytes, 0);
        assert_eq!(&d.pixels[0..4], &[1, 2, 3, 4]);
        assert_eq!(&d.pixels[4..8], &[5, 6, 7, 8]);
    }

    #[test]
    fn raw_8bpp_with_palette_remaps() {
        // 4x1 8bpp, palette with 2 entries.
        let mut buf = Vec::new();
        write_u32(&mut buf, IMG_MAGIC);
        let ds_off = buf.len();
        write_u32(&mut buf, 0);
        write_u16(&mut buf, 0x8008); // flags: bpp=8, has_palette
        write_u16(&mut buf, 2); // palette_count = 2
        // RGB triplets — map_color is a closure that returns `r + 100`
        buf.extend_from_slice(&[10, 0, 0]); // -> 110
        buf.extend_from_slice(&[20, 0, 0]); // -> 120
        write_u16(&mut buf, 4);
        write_u16(&mut buf, 1);
        // pixels: [0, 1, 2, 1]
        //   lut[0] = 0, lut[1] = 110, lut[2] = 120, lut[3..] = 0
        // remapped: [0, 110, 120, 110]
        buf.extend_from_slice(&[0, 1, 2, 1]);
        let ds = buf.len() as u32;
        buf[ds_off..ds_off + 4].copy_from_slice(&ds.to_le_bytes());

        let d = img_decode(&buf, false, |rgb| ((rgb & 0xFF) as u8) + 100).unwrap();
        assert_eq!(d.palette_rgb_bytes, 6);
        assert_eq!(&d.pixels[0..4], &[0, 110, 120, 110]);
    }

    #[test]
    fn raw_1bpp_collision() {
        // width=16, height=2, 1bpp, no palette. bytes_per_row = 2,
        // row_stride = ((16+7)/8 + 3) & !3 = (2 + 3) & !3 = 4.
        let mut buf = Vec::new();
        write_u32(&mut buf, IMG_MAGIC);
        let ds_off = buf.len();
        write_u32(&mut buf, 0);
        write_u16(&mut buf, 0x0001); // bpp=1
        write_u16(&mut buf, 16);
        write_u16(&mut buf, 2);
        buf.extend_from_slice(&[0xAA, 0x55]); // row 0
        buf.extend_from_slice(&[0xF0, 0x0F]); // row 1
        let ds = buf.len() as u32;
        buf[ds_off..ds_off + 4].copy_from_slice(&ds.to_le_bytes());

        let d = img_decode(&buf, false, |_| 0).unwrap();
        assert_eq!(d.bpp, 1);
        assert_eq!(d.row_stride, 4);
        assert_eq!(&d.pixels[0..2], &[0xAA, 0x55]);
        assert_eq!(&d.pixels[2..4], &[0, 0]); // stride padding
        assert_eq!(&d.pixels[4..6], &[0xF0, 0x0F]);
    }

    #[test]
    fn unsupported_bpp() {
        let mut buf = Vec::new();
        write_u32(&mut buf, IMG_MAGIC);
        write_u32(&mut buf, 0);
        // bpp=9 (unsupported). Must avoid the extended-header bits
        // (0x3FF6) in the flags word, which rules out e.g. 2, 4, 16.
        write_u16(&mut buf, 0x0009);
        write_u16(&mut buf, 1);
        write_u16(&mut buf, 1);
        let err = img_decode(&buf, false, |_| 0).unwrap_err();
        assert_eq!(err, ImgDecodeError::UnsupportedBpp(9));
    }

    #[test]
    fn lzss_compressed_path() {
        // 1 row, 4 bytes wide, 8bpp, LZSS + palette.
        // lut[1..=3] set via palette; LZSS stream: literals 1,2,3, short
        // back-ref (nibble=1, dist enc=2) for "ABC" style, terminator.
        // We'll just encode a pure-literal LZSS stream for simplicity.
        let mut buf = Vec::new();
        write_u32(&mut buf, IMG_MAGIC);
        let ds_off = buf.len();
        write_u32(&mut buf, 0);
        write_u16(&mut buf, 0xC008); // bpp=8, has_palette, lzss
        write_u16(&mut buf, 3); // palette_count=3
        // RGB triplets -> map_color returns the red byte itself
        buf.extend_from_slice(&[0xAA, 0, 0]);
        buf.extend_from_slice(&[0xBB, 0, 0]);
        buf.extend_from_slice(&[0xCC, 0, 0]);
        write_u16(&mut buf, 4);
        write_u16(&mut buf, 1);
        // LZSS: 4 literals (1,2,3,1), terminator (0x80, 0x00)
        buf.extend_from_slice(&[1, 2, 3, 1, 0x80, 0x00]);
        let ds = buf.len() as u32;
        buf[ds_off..ds_off + 4].copy_from_slice(&ds.to_le_bytes());

        let d = img_decode(&buf, false, |rgb| (rgb & 0xFF) as u8).unwrap();
        // lut: [0, 0xAA, 0xBB, 0xCC, 0, ...]
        assert_eq!(&d.pixels[0..4], &[0xAA, 0xBB, 0xCC, 0xAA]);
    }

    #[test]
    fn align_consumes_padding() {
        // Construct an IMG where after header reads the position is not
        // 4-aligned. flags(no palette) + width + height = 10 bytes past
        // magic+data_size (pos 8). Headers put us at pos 14, then if
        // align=true we consume 2 bytes to reach 16. Use 1bpp so the
        // raw-row copy reads a known pattern.
        //
        // Layout:
        //   [0..4]   magic
        //   [4..8]   data_size
        //   [8..10]  flags (0x0001)
        //   [10..12] width (8)
        //   [12..14] height (1)
        //   [14..16] padding (0xCC, 0xCC — should be skipped)
        //   [16..17] pixel row (0x5A)
        let mut buf = Vec::new();
        write_u32(&mut buf, IMG_MAGIC);
        let ds_off = buf.len();
        write_u32(&mut buf, 0);
        write_u16(&mut buf, 0x0001);
        write_u16(&mut buf, 8);
        write_u16(&mut buf, 1);
        buf.extend_from_slice(&[0xCC, 0xCC]);
        buf.extend_from_slice(&[0x5A]);
        let ds = buf.len() as u32;
        buf[ds_off..ds_off + 4].copy_from_slice(&ds.to_le_bytes());

        let d = img_decode(&buf, true, |_| 0).unwrap();
        assert_eq!(d.pixels[0], 0x5A);
    }

    #[test]
    fn headerless_format_basic() {
        // 4x2 8bpp cached image, 2 palette entries. Header layout:
        //   [0..10]   skipped
        //   [10..12]  palette_count = 2
        //   [12..18]  RGB triplets (2 * 3 bytes)
        //   [18..20]  width = 4
        //   [20..22]  height = 2
        //   wh_start = 0x0C + 6 = 0x12
        //   aligned_offset = (0x12 + 7) & !3 = 0x14
        //   pixels start at 0x14
        let mut buf = vec![0u8; 0x14];
        // palette_count
        buf[0x0A..0x0C].copy_from_slice(&2u16.to_le_bytes());
        // RGB: (10,0,0) and (20,0,0)
        buf[0x0C..0x0F].copy_from_slice(&[10, 0, 0]);
        buf[0x0F..0x12].copy_from_slice(&[20, 0, 0]);
        // width, height
        buf[0x12..0x14].copy_from_slice(&4u16.to_le_bytes());
        buf.extend_from_slice(&2u16.to_le_bytes());
        // Hmm wait — height overlaps aligned_offset. Rebuild cleanly.
        let mut buf = vec![0u8; 0x14];
        buf[0x0A..0x0C].copy_from_slice(&2u16.to_le_bytes());
        buf[0x0C..0x0F].copy_from_slice(&[10, 0, 0]);
        buf[0x0F..0x12].copy_from_slice(&[20, 0, 0]);
        buf[0x12..0x14].copy_from_slice(&4u16.to_le_bytes());
        // height at [0x14..0x16], but pixel data starts at aligned_offset=0x14.
        // The original code's +5 formula results in aligned_offset that MAY overlap
        // height — in this test case, let's recompute: wh_start = 0x12, +7 = 0x19,
        // &!3 = 0x18. So aligned_offset = 0x18, not 0x14. Fix:
        let mut buf = vec![0u8; 0x18];
        buf[0x0A..0x0C].copy_from_slice(&2u16.to_le_bytes());
        buf[0x0C..0x0F].copy_from_slice(&[10, 0, 0]);
        buf[0x0F..0x12].copy_from_slice(&[20, 0, 0]);
        buf[0x12..0x14].copy_from_slice(&4u16.to_le_bytes());
        buf[0x14..0x16].copy_from_slice(&2u16.to_le_bytes());
        // bytes 0x16..0x18 are padding
        // pixels: row0 [0,1,2,1], row1 [1,1,0,2]
        buf.extend_from_slice(&[0, 1, 2, 1]);
        buf.extend_from_slice(&[1, 1, 0, 2]);

        let d = img_decode_headerless(&buf, |rgb| ((rgb & 0xFF) as u8) + 100).unwrap();
        assert_eq!(d.bpp, 8);
        assert_eq!(d.width, 4);
        assert_eq!(d.height, 2);
        assert_eq!(d.row_stride, 4);
        assert_eq!(d.palette_rgb_bytes, 6);
        // lut[0]=0, lut[1]=110, lut[2]=120, and remap_width = (4/4)*4 = 4
        assert_eq!(&d.pixels[0..4], &[0, 110, 120, 110]);
        assert_eq!(&d.pixels[4..8], &[110, 110, 0, 120]);
    }
}
