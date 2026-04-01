//! `.spr` sprite format parser.
//!
//! WA's sprite format stores paletted animated sprites with per-frame metadata.
//! This module provides both a clean Rust parser (`ParsedSprite`) for tooling
//! and shared metadata extraction (`parse_spr_header`) used by WormKit hook
//! replacements.

use crate::render::sprite::SpriteFrame;

/// Errors from `.spr` parsing.
#[derive(Debug)]
pub enum SprError {
    /// Data too short to contain required fields.
    TooShort {
        expected: usize,
        actual: usize,
    },
    /// Internal structure is inconsistent.
    InvalidData(&'static str),
}

impl core::fmt::Display for SprError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SprError::TooShort { expected, actual } => {
                write!(f, "spr data too short: need {} bytes, got {}", expected, actual)
            }
            SprError::InvalidData(msg) => write!(f, "invalid spr data: {}", msg),
        }
    }
}

/// Parsed metadata and byte offsets into raw `.spr` data.
///
/// Produced by `parse_spr_header`. Contains all scalar fields and the byte
/// offsets needed to locate palette, frame, and bitmap data within the raw
/// buffer. No heap allocation — just arithmetic on the input slice.
#[derive(Debug, Clone)]
pub struct SprHeader {
    /// Total payload size from `.spr` header (+0x04). Used for global counter.
    pub data_size: u32,
    /// Header flags from `.spr` (+0x08).
    pub header_flags: u16,
    /// Number of RGB palette entries.
    pub palette_count: u16,
    /// Unknown field stored at sprite+0x08.
    pub unknown_08: u16,
    /// Animation speed (frames per second).
    pub fps: u16,
    /// Sprite flags.
    pub flags: u16,
    /// Sprite width in pixels.
    pub width: u16,
    /// Sprite height in pixels.
    pub height: u16,
    /// Frame count (1 if scaled).
    pub frame_count: u16,
    /// Max frames (same as frame_count after processing).
    pub max_frames: u16,
    /// Scale X: `((raw >> 8 & 0x7F) << 16) >> 5`, or 0 if not scaled.
    pub scale_x: u32,
    /// Scale Y: `((raw & 0x7F) << 16) >> 5`, or 0 if not scaled.
    pub scale_y: u32,
    /// True if the frame_count word encoded scaling instead of a count.
    pub is_scaled: bool,
    /// Number of secondary frames (0 if header_flags & 0x4000 is clear).
    pub secondary_frame_count: u16,
    /// Byte offset of RGB palette data (3 bytes per entry).
    pub palette_offset: usize,
    /// Byte offset of secondary frame table (0 if none).
    pub secondary_frame_offset: usize,
    /// Byte offset of main SpriteFrame metadata array.
    pub frame_meta_offset: usize,
    /// Byte offset of bitmap pixel data.
    pub bitmap_offset: usize,
    /// Size of bitmap pixel data in bytes.
    pub bitmap_size: usize,
}

/// Align `offset` up to the next 4-byte boundary.
fn align4(offset: usize) -> usize {
    (offset + 3) & !3
}

/// Parse `.spr` header and compute data region offsets.
///
/// This is a pure function with no WA dependencies. Both `ParsedSprite::parse`
/// and the WormKit ProcessSprite hook build on this.
pub fn parse_spr_header(data: &[u8]) -> Result<SprHeader, SprError> {
    // Minimum: 4 (unused) + 4 (data_size) + 2 (header_flags) + 2 (palette_count) = 12
    if data.len() < 12 {
        return Err(SprError::TooShort { expected: 12, actual: data.len() });
    }

    let data_size = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let header_flags = u16::from_le_bytes([data[8], data[9]]);
    let palette_count = u16::from_le_bytes([data[10], data[11]]);

    let palette_offset = 12; // 0x0C
    let palette_end = palette_offset + palette_count as usize * 3;

    if data.len() < palette_end {
        return Err(SprError::TooShort { expected: palette_end, actual: data.len() });
    }

    let mut cursor = palette_end;

    // Secondary frame table (if header_flags & 0x4000)
    let mut secondary_frame_count: u16 = 0;
    let mut secondary_frame_offset: usize = 0;

    if header_flags & 0x4000 != 0 {
        if data.len() < cursor + 2 {
            return Err(SprError::TooShort { expected: cursor + 2, actual: data.len() });
        }
        secondary_frame_count = u16::from_le_bytes([data[cursor], data[cursor + 1]]);
        cursor = align4(cursor + 2);
        secondary_frame_offset = cursor;
        cursor += secondary_frame_count as usize * 12;
    }

    // Main frame header (12 bytes)
    if data.len() < cursor + 12 {
        return Err(SprError::TooShort { expected: cursor + 12, actual: data.len() });
    }

    let unknown_08 = u16::from_le_bytes([data[cursor], data[cursor + 1]]);
    let fps = u16::from_le_bytes([data[cursor + 2], data[cursor + 3]]);
    let flags = u16::from_le_bytes([data[cursor + 4], data[cursor + 5]]);
    let width = u16::from_le_bytes([data[cursor + 6], data[cursor + 7]]);
    let height = u16::from_le_bytes([data[cursor + 8], data[cursor + 9]]);
    let frame_count_raw = u16::from_le_bytes([data[cursor + 10], data[cursor + 11]]);

    cursor += 12;

    // Scale processing
    let (frame_count, max_frames, scale_x, scale_y, is_scaled) =
        if (frame_count_raw as i16) < 0 {
            let sx = (((frame_count_raw >> 8) & 0x7F) as u32) << 16 >> 5;
            let sy = ((frame_count_raw & 0x7F) as u32) << 16 >> 5;
            (1u16, 1u16, sx, sy, true)
        } else {
            (frame_count_raw, frame_count_raw, 0u32, 0u32, false)
        };

    // Frame metadata (aligned to 4 bytes)
    let frame_meta_offset = align4(cursor);
    let bitmap_offset = frame_meta_offset + frame_count as usize * 12;

    let bitmap_size = if data.len() > bitmap_offset {
        data.len() - bitmap_offset
    } else {
        0
    };

    Ok(SprHeader {
        data_size,
        header_flags,
        palette_count,
        unknown_08,
        fps,
        flags,
        width,
        height,
        frame_count,
        max_frames,
        scale_x,
        scale_y,
        is_scaled,
        secondary_frame_count,
        palette_offset,
        secondary_frame_offset,
        frame_meta_offset,
        bitmap_offset,
        bitmap_size,
    })
}

/// A fully parsed `.spr` sprite with owned data.
///
/// Contains raw RGB palette entries (not remapped to WA's display palette),
/// raw bitmap pixel indices, and frame metadata. Suitable for standalone
/// tooling, tests, and debugging.
///
/// For WormKit hook replacements, `parse_spr_header` is used directly
/// with in-place buffer operations instead.
#[derive(Debug, Clone)]
pub struct ParsedSprite {
    /// Unknown field at sprite+0x08.
    pub unknown_08: u16,
    /// Animation frames per second.
    pub fps: u16,
    /// Sprite flags.
    pub flags: u16,
    /// Header flags from `.spr` file.
    pub header_flags: u16,
    /// Sprite width in pixels.
    pub width: u16,
    /// Sprite height in pixels.
    pub height: u16,
    /// Number of animation frames.
    pub frame_count: u16,
    /// Maximum frame count.
    pub max_frames: u16,
    /// Scale X (`(raw << 16) >> 5`), or 0 if not scaled.
    pub scale_x: u32,
    /// Scale Y (`(raw << 16) >> 5`), or 0 if not scaled.
    pub scale_y: u32,
    /// Whether the sprite uses scaling instead of animation.
    pub is_scaled: bool,
    /// RGB palette entries (3 bytes each). NOT remapped to display palette.
    pub palette: Vec<[u8; 3]>,
    /// Per-frame metadata (bounding box + bitmap offset).
    pub frames: Vec<SpriteFrame>,
    /// Secondary frame table (non-empty iff header_flags & 0x4000).
    pub secondary_frames: Vec<SpriteFrame>,
    /// Bitmap pixel data — 8-bit indices into the sprite's local palette.
    pub bitmap: Vec<u8>,
    /// Total data size from `.spr` header (for global counter updates).
    pub data_size: u32,
}

impl ParsedSprite {
    /// Parse a `.spr` file from raw bytes.
    ///
    /// Returns owned data with no WA runtime dependencies.
    pub fn parse(data: &[u8]) -> Result<Self, SprError> {
        let hdr = parse_spr_header(data)?;

        // Extract palette RGB triples
        let mut palette = Vec::with_capacity(hdr.palette_count as usize);
        let mut offset = hdr.palette_offset;
        for _ in 0..hdr.palette_count {
            palette.push([data[offset], data[offset + 1], data[offset + 2]]);
            offset += 3;
        }

        // Extract secondary frames
        let mut secondary_frames = Vec::new();
        if hdr.secondary_frame_count > 0 {
            secondary_frames.reserve(hdr.secondary_frame_count as usize);
            let mut off = hdr.secondary_frame_offset;
            for _ in 0..hdr.secondary_frame_count {
                secondary_frames.push(read_sprite_frame(data, off));
                off += 12;
            }
        }

        // Extract main frames
        let mut frames = Vec::with_capacity(hdr.frame_count as usize);
        let mut off = hdr.frame_meta_offset;
        for _ in 0..hdr.frame_count {
            frames.push(read_sprite_frame(data, off));
            off += 12;
        }

        // Extract bitmap data
        let bitmap_end = data.len().min(hdr.bitmap_offset + hdr.bitmap_size);
        let bitmap = data[hdr.bitmap_offset..bitmap_end].to_vec();

        Ok(ParsedSprite {
            unknown_08: hdr.unknown_08,
            fps: hdr.fps,
            flags: hdr.flags,
            header_flags: hdr.header_flags,
            width: hdr.width,
            height: hdr.height,
            frame_count: hdr.frame_count,
            max_frames: hdr.max_frames,
            scale_x: hdr.scale_x,
            scale_y: hdr.scale_y,
            is_scaled: hdr.is_scaled,
            palette,
            frames,
            secondary_frames,
            bitmap,
            data_size: hdr.data_size,
        })
    }
}

/// Read a SpriteFrame from 12 bytes at the given offset.
fn read_sprite_frame(data: &[u8], off: usize) -> SpriteFrame {
    SpriteFrame {
        bitmap_offset: u32::from_le_bytes([data[off], data[off+1], data[off+2], data[off+3]]),
        start_x: u16::from_le_bytes([data[off+4], data[off+5]]),
        start_y: u16::from_le_bytes([data[off+6], data[off+7]]),
        end_x: u16::from_le_bytes([data[off+8], data[off+9]]),
        end_y: u16::from_le_bytes([data[off+10], data[off+11]]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal single-frame .spr with given dimensions and palette.
    fn make_spr(width: u16, height: u16, palette_count: u16, frame_count: u16) -> Vec<u8> {
        let mut data = Vec::new();

        // +0x00: unused magic
        data.extend_from_slice(&0u32.to_le_bytes());
        // +0x04: data_size (we'll fix up later)
        let data_size_pos = data.len();
        data.extend_from_slice(&0u32.to_le_bytes());
        // +0x08: header_flags
        data.extend_from_slice(&0u16.to_le_bytes());
        // +0x0A: palette_entry_count
        data.extend_from_slice(&palette_count.to_le_bytes());
        // +0x0C: palette RGB entries (all zeros)
        for _ in 0..palette_count {
            data.extend_from_slice(&[0u8; 3]);
        }

        // Main frame header (12 bytes)
        data.extend_from_slice(&0u16.to_le_bytes()); // unknown_08
        data.extend_from_slice(&10u16.to_le_bytes()); // fps = 10
        data.extend_from_slice(&0u16.to_le_bytes()); // flags
        data.extend_from_slice(&width.to_le_bytes());
        data.extend_from_slice(&height.to_le_bytes());
        data.extend_from_slice(&frame_count.to_le_bytes());

        // Align to 4 bytes
        while data.len() % 4 != 0 {
            data.push(0);
        }

        // Frame metadata (12 bytes per frame)
        for i in 0..frame_count {
            let offset = i as u32 * width as u32 * height as u32;
            data.extend_from_slice(&offset.to_le_bytes()); // bitmap_offset
            data.extend_from_slice(&0u16.to_le_bytes()); // start_x
            data.extend_from_slice(&0u16.to_le_bytes()); // start_y
            data.extend_from_slice(&width.to_le_bytes()); // end_x
            data.extend_from_slice(&height.to_le_bytes()); // end_y
        }

        // Bitmap data (width * height * frame_count bytes)
        let bitmap_bytes = width as usize * height as usize * frame_count as usize;
        data.resize(data.len() + bitmap_bytes, 0);

        // Fix up data_size (everything from +4 onward)
        let total = (data.len() - 4) as u32;
        data[data_size_pos..data_size_pos + 4].copy_from_slice(&total.to_le_bytes());

        data
    }

    #[test]
    fn parse_single_frame() {
        let data = make_spr(32, 16, 4, 1);
        let hdr = parse_spr_header(&data).unwrap();

        assert_eq!(hdr.width, 32);
        assert_eq!(hdr.height, 16);
        assert_eq!(hdr.fps, 10);
        assert_eq!(hdr.palette_count, 4);
        assert_eq!(hdr.frame_count, 1);
        assert!(!hdr.is_scaled);
        assert_eq!(hdr.scale_x, 0);
        assert_eq!(hdr.scale_y, 0);
        assert_eq!(hdr.secondary_frame_count, 0);
        assert_eq!(hdr.palette_offset, 12);
        assert!(hdr.frame_meta_offset > 0);
        assert!(hdr.bitmap_offset > hdr.frame_meta_offset);
    }

    #[test]
    fn parse_multi_frame() {
        let data = make_spr(8, 8, 2, 5);
        let hdr = parse_spr_header(&data).unwrap();

        assert_eq!(hdr.frame_count, 5);
        assert_eq!(hdr.max_frames, 5);
        // 5 frames * 12 bytes each = 60 bytes of frame metadata
        assert_eq!(hdr.bitmap_offset - hdr.frame_meta_offset, 60);
    }

    #[test]
    fn parse_scaled_sprite() {
        // frame_count_raw = 0xA050: bit15 set, scale_x_raw = 0x20, scale_y_raw = 0x50
        let mut data = make_spr(32, 32, 0, 1);
        // Overwrite frame_count_raw in the main frame header
        // palette_count = 0, so palette_end = 12
        // main frame header starts at 12, frame_count is at offset 10 within it = byte 22
        data[22] = 0x50; // low byte
        data[23] = 0xA0; // high byte (bit 15 set)

        let hdr = parse_spr_header(&data).unwrap();

        assert!(hdr.is_scaled);
        assert_eq!(hdr.frame_count, 1);
        assert_eq!(hdr.max_frames, 1);
        // scale_x = (0x20 << 16) >> 5 = 0x200000 >> 5 = 0x10000
        assert_eq!(hdr.scale_x, (0x20u32 << 16) >> 5);
        // scale_y = (0x50 << 16) >> 5 = 0x500000 >> 5 = 0x28000
        assert_eq!(hdr.scale_y, (0x50u32 << 16) >> 5);
    }

    #[test]
    fn parse_too_short() {
        assert!(parse_spr_header(&[0; 4]).is_err());
    }

    #[test]
    fn parsed_sprite_single_frame() {
        let mut data = make_spr(4, 4, 3, 1);
        // Fill palette with known RGB values
        data[12] = 0xFF; data[13] = 0x00; data[14] = 0x00; // entry 0: red
        data[15] = 0x00; data[16] = 0xFF; data[17] = 0x00; // entry 1: green
        data[18] = 0x00; data[19] = 0x00; data[20] = 0xFF; // entry 2: blue

        let parsed = ParsedSprite::parse(&data).unwrap();
        assert_eq!(parsed.width, 4);
        assert_eq!(parsed.height, 4);
        assert_eq!(parsed.palette.len(), 3);
        assert_eq!(parsed.palette[0], [0xFF, 0x00, 0x00]);
        assert_eq!(parsed.palette[1], [0x00, 0xFF, 0x00]);
        assert_eq!(parsed.palette[2], [0x00, 0x00, 0xFF]);
        assert_eq!(parsed.frames.len(), 1);
        assert_eq!(parsed.bitmap.len(), 16); // 4*4 pixels
    }

    #[test]
    fn parsed_sprite_secondary_frames() {
        let mut data = Vec::new();

        // Header
        data.extend_from_slice(&0u32.to_le_bytes()); // unused
        data.extend_from_slice(&0u32.to_le_bytes()); // data_size (fix later)
        data.extend_from_slice(&0x4000u16.to_le_bytes()); // header_flags: secondary frames
        data.extend_from_slice(&0u16.to_le_bytes()); // palette_count = 0

        // Secondary frame table
        data.extend_from_slice(&2u16.to_le_bytes()); // 2 secondary frames
        // Align to 4 bytes
        while data.len() % 4 != 0 { data.push(0); }
        // 2 secondary SpriteFrame entries (12 bytes each)
        for _ in 0..2 {
            data.extend_from_slice(&[0u8; 12]);
        }

        // Main frame header
        data.extend_from_slice(&0u16.to_le_bytes()); // unknown_08
        data.extend_from_slice(&15u16.to_le_bytes()); // fps
        data.extend_from_slice(&0u16.to_le_bytes()); // flags
        data.extend_from_slice(&8u16.to_le_bytes()); // width
        data.extend_from_slice(&8u16.to_le_bytes()); // height
        data.extend_from_slice(&1u16.to_le_bytes()); // frame_count

        while data.len() % 4 != 0 { data.push(0); }
        // 1 main SpriteFrame
        data.extend_from_slice(&0u32.to_le_bytes()); // bitmap_offset
        data.extend_from_slice(&0u16.to_le_bytes()); // start_x
        data.extend_from_slice(&0u16.to_le_bytes()); // start_y
        data.extend_from_slice(&8u16.to_le_bytes()); // end_x
        data.extend_from_slice(&8u16.to_le_bytes()); // end_y

        // Bitmap (8*8 = 64 bytes)
        data.resize(data.len() + 64, 0);

        // Fix data_size
        let total = (data.len() - 4) as u32;
        data[4..8].copy_from_slice(&total.to_le_bytes());

        let parsed = ParsedSprite::parse(&data).unwrap();
        assert_eq!(parsed.secondary_frames.len(), 2);
        assert_eq!(parsed.frames.len(), 1);
        assert_eq!(parsed.header_flags, 0x4000);
    }
}
