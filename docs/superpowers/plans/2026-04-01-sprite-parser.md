# Sprite Parser Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port WA's `.spr` sprite parser to Rust — a clean `ParsedSprite` type for tooling, plus hook replacements for `ConstructSprite` and `ProcessSprite`.

**Architecture:** A shared `parse_spr_header()` function extracts metadata and data offsets from raw `.spr` bytes. `ParsedSprite::parse()` builds on it to produce an owned Rust struct with `Vec` data. The hook replacements use `parse_spr_header()` directly, operating on the raw buffer in-place for palette remapping and bitmap index translation.

**Tech Stack:** Rust, `openwa-core` (types/parser), `openwa-dll` (hooks), `minhook` (hooking), WA.exe runtime (PaletteContext__MapColor)

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/openwa-core/src/render/spr.rs` (create) | `SprHeader`, `SprError`, `parse_spr_header()`, `ParsedSprite`, `ParsedSprite::parse()` |
| `crates/openwa-core/src/render/mod.rs` (modify) | Add `pub mod spr;` and re-exports |
| `crates/openwa-core/src/address.rs` (modify) | Add `PALETTE_CONTEXT_MAP_COLOR`, `DISPLAYGFX_VTABLE` |
| `crates/openwa-dll/src/replacements/sprite.rs` (create) | Hook trampolines + `install()` for ConstructSprite, ProcessSprite |
| `crates/openwa-dll/src/replacements/mod.rs` (modify) | Wire `sprite::install()` |

## Reference: .spr Binary Format

Derived from Ghidra disassembly of ProcessSprite (0x4FAB80). All offsets relative to start of raw data.

```
+0x00  u32    (unused / magic — not validated by ProcessSprite)
+0x04  u32    data_size — total payload size, added to global counter
+0x08  u16    header_flags — stored at sprite+0x14
+0x0A  u16    palette_entry_count
+0x0C  3*N    palette RGB entries (3 bytes each: R, G, B)

[After palette, if header_flags & 0x4000:]
       u16    secondary_frame_count
       align cursor to 4-byte boundary (relative to data start)
       N*12   secondary SpriteFrame entries (12 bytes each)

[Main frame header — 12 bytes:]
       u16    unknown_08 → sprite+0x08
       u16    fps → sprite+0x0A
       u16    flags → sprite+0x10
       u16    width → sprite+0x0C
       u16    height → sprite+0x0E
       u16    frame_count_raw → sprite+0x12/0x16 (negative = scaling)

       align cursor to 4-byte boundary (relative to data start)
       frame_count * 12 bytes of SpriteFrame metadata
       remaining bytes: bitmap pixel data (8-bit palette indices)
```

**Scaling:** If frame_count_raw has bit 15 set (negative as i16):
- `scale_x = ((frame_count_raw >> 8) & 0x7F) << 16 >> 5` (arithmetic shift)
- `scale_y = (frame_count_raw & 0x7F) << 16 >> 5`
- Actual frame_count = 1, is_scaled = true

**Palette remapping (WA runtime only):**
- `palette_data_ptr[0] = 0` (transparent index)
- For each entry 1..N: `palette_data_ptr[i] = PaletteContext__MapColor(this=ECX, rgb_u32)`
- Then ALL bitmap bytes remapped: `pixel = palette_data_ptr[pixel]`
- Bitmap remapping only happens when header_flags & 0x4000 is NOT set

## Reference: Key WA Functions

| Address | Convention | Description |
|---|---|---|
| 0x4FAA30 | usercall(EAX=sprite, ECX=context), plain RET | ConstructSprite |
| 0x4FAB80 | usercall(EAX=sprite, ECX=palette_ctx) + 1 stack(raw_data), RET 0x4 | ProcessSprite |
| 0x5412B0 | thiscall(ECX=palette_ctx, stack=rgb_u32), RET 0x4, returns u8 | PaletteContext__MapColor |
| 0x66418C | data | Sprite vtable address |
| 0x664144 | data | DisplayGfx vtable address |

---

### Task 1: Add Address Constants

**Files:**
- Modify: `crates/openwa-core/src/address.rs`

- [ ] **Step 1: Add PALETTE_CONTEXT_MAP_COLOR and DISPLAYGFX_VTABLE**

In `address.rs`, find the `class "Sprite"` block and add the DisplayGfx vtable. Find the DDDisplay section and add the palette function:

```rust
// In the class "Sprite" block, after PROCESS_SPRITE:
// (no change needed here, but add DisplayGfx vtable nearby)

// Add to the DDDisplay section (near PALETTE_CONTEXT_INIT):
/// PaletteContext__MapColor — thiscall(ECX=ctx, stack=rgb), RET 0x4, returns u8
fn/Thiscall PALETTE_CONTEXT_MAP_COLOR = 0x0054_12B0;

// Add DisplayGfx vtable as a standalone entry near SPRITE_VTABLE:
// In the class "Sprite" block:
/// DisplayGfx vtable (embedded in Sprite at +0x34)
vtable DISPLAYGFX_VTABLE = 0x0066_4144;
```

Specifically, in the `class "Sprite"` block after `fn/Usercall PROCESS_SPRITE`, add:

```rust
            /// DisplayGfx vtable (embedded in Sprite at +0x34)
            vtable DISPLAYGFX_VTABLE = 0x0066_4144;
```

And near `PALETTE_CONTEXT_INIT` in the DDDisplay section, add:

```rust
        /// PaletteContext__MapColor — thiscall(palette_ctx, rgb_u32), returns nearest palette index
        fn/Thiscall PALETTE_CONTEXT_MAP_COLOR = 0x0054_12B0;
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p openwa-core 2>&1 | head -5`
Expected: Compiles without errors.

- [ ] **Step 3: Commit**

```bash
git add crates/openwa-core/src/address.rs
git commit -m "feat: add PALETTE_CONTEXT_MAP_COLOR and DISPLAYGFX_VTABLE addresses"
```

---

### Task 2: Create spr.rs — SprHeader and parse_spr_header()

**Files:**
- Create: `crates/openwa-core/src/render/spr.rs`
- Modify: `crates/openwa-core/src/render/mod.rs`

- [ ] **Step 1: Write the test for parse_spr_header**

Create `crates/openwa-core/src/render/spr.rs` with the error type, header struct, and a test:

```rust
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

    // bitmap_size: remaining bytes from data_size
    // ProcessSprite computes: data_size - (bitmap_offset - data_start)
    // But data_size is measured from offset +4, and data_start is offset 0.
    // Actually WA computes: bitmap_dword_count = (data_size - bitmap_ptr + raw_data_ptr + 3) / 4
    // which simplifies to: (data_size + 4 - bitmap_offset + 3) / 4 * 4
    // For our purposes: bitmap extends from bitmap_offset to the end of known data.
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
        // frame_count_raw = 0x8040: bit15 set, scale_x_raw = 0, scale_y_raw = 0x40
        // Actually: high byte = 0x80, (0x80 >> 0) & 0x7F for scale from bits 8..14
        // Let's use 0xA050: high byte 0xA0, low byte 0x50
        // scale_x_raw = (0xA050 >> 8) & 0x7F = 0xA0 & 0x7F = 0x20
        // scale_y_raw = 0xA050 & 0x7F = 0x50
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
}
```

- [ ] **Step 2: Add module to render/mod.rs**

In `crates/openwa-core/src/render/mod.rs`, add:

```rust
pub mod spr;
```

And add re-exports:

```rust
pub use spr::{ParsedSprite, SprError, SprHeader};
```

(The `ParsedSprite` re-export will fail initially since we haven't defined it yet — that's fine, comment it out or add it in Task 3.)

- [ ] **Step 3: Run tests**

Run: `cargo test -p openwa-core -- spr::tests`
Expected: All 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/openwa-core/src/render/spr.rs crates/openwa-core/src/render/mod.rs
git commit -m "feat: add .spr header parser with SprHeader and parse_spr_header()"
```

---

### Task 3: Add ParsedSprite and ParsedSprite::parse()

**Files:**
- Modify: `crates/openwa-core/src/render/spr.rs`
- Modify: `crates/openwa-core/src/render/mod.rs`

- [ ] **Step 1: Write failing test for ParsedSprite::parse**

Add to the `tests` module in `spr.rs`:

```rust
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
```

- [ ] **Step 2: Implement ParsedSprite**

Add above the `#[cfg(test)]` block in `spr.rs`:

```rust
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
```

- [ ] **Step 3: Update render/mod.rs re-exports**

```rust
pub use spr::{ParsedSprite, SprError, SprHeader};
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p openwa-core -- spr::tests`
Expected: All 6 tests pass (4 from Task 2 + 2 new).

- [ ] **Step 5: Commit**

```bash
git add crates/openwa-core/src/render/spr.rs crates/openwa-core/src/render/mod.rs
git commit -m "feat: add ParsedSprite type with owned .spr parser"
```

---

### Task 4: ConstructSprite Hook Replacement

**Files:**
- Create: `crates/openwa-dll/src/replacements/sprite.rs`

- [ ] **Step 1: Create sprite.rs with ConstructSprite hook**

Create `crates/openwa-dll/src/replacements/sprite.rs`:

```rust
//! Sprite loading hook replacements.
//!
//! Replaces ConstructSprite (0x4FAA30) and ProcessSprite (0x4FAB80) with
//! Rust implementations. Uses `parse_spr_header` from openwa-core for
//! format parsing.

use openwa_core::address::va;
use openwa_core::rebase::rb;

use crate::hook::{self, usercall_trampoline};

// ---------------------------------------------------------------------------
// ConstructSprite (0x4FAA30) — usercall(EAX=sprite, ECX=context), plain RET
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_construct_sprite; impl_fn = construct_sprite_impl;
    regs = [eax, ecx]);

unsafe extern "cdecl" fn construct_sprite_impl(sprite: u32, context: u32) {
    let p = sprite as *mut u8;

    // Zero the entire 0x70-byte struct first
    core::ptr::write_bytes(p, 0, 0x70);

    // Vtable
    *(p as *mut u32) = rb(va::SPRITE_VTABLE);
    // Context pointer (+0x04)
    *(p.add(0x04) as *mut u32) = context;
    // DisplayGfx vtable (+0x34)
    *(p.add(0x34) as *mut u32) = rb(va::DISPLAYGFX_VTABLE);
    // _unknown_38 = 1 (+0x38)
    *(p.add(0x38) as *mut u32) = 1;
    // _unknown_40 = 8 (+0x40)
    *(p.add(0x40) as *mut u32) = 8;
}

// ---------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------

pub fn install() -> Result<(), String> {
    unsafe {
        let _ = hook::install(
            "ConstructSprite",
            va::CONSTRUCT_SPRITE,
            trampoline_construct_sprite as *const (),
        )?;
    }
    Ok(())
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p openwa-dll --release 2>&1 | tail -5`
Expected: Compiles (sprite.rs is not yet in mod.rs, so it won't be included — this just checks syntax).

Actually, it won't compile unless added to mod.rs. Skip to step 3.

- [ ] **Step 3: Add to mod.rs and verify**

In `crates/openwa-dll/src/replacements/mod.rs`, add `mod sprite;` and add `sprite::install()?;` in `install_all()` after `render::install()?;`.

Run: `cargo build -p openwa-dll --release 2>&1 | tail -5`
Expected: Compiles without errors.

- [ ] **Step 4: Commit**

```bash
git add crates/openwa-dll/src/replacements/sprite.rs crates/openwa-dll/src/replacements/mod.rs
git commit -m "feat: add ConstructSprite hook replacement"
```

---

### Task 5: ProcessSprite Hook Replacement

**Files:**
- Modify: `crates/openwa-dll/src/replacements/sprite.rs`

This is the most complex task. ProcessSprite:
1. Reads header metadata via `parse_spr_header`
2. Builds a palette lookup table in-place (calling PaletteContext__MapColor)
3. Sets all sprite struct fields + pointers into the raw buffer
4. Remaps bitmap pixels through the lookup table (when no secondary frames)
5. Computes and updates 4 global counters

- [ ] **Step 1: Add PaletteContext__MapColor bridge**

Add to `sprite.rs`, above the `install()` function:

```rust
// ---------------------------------------------------------------------------
// PaletteContext__MapColor bridge (0x5412B0)
// thiscall(ECX=palette_ctx, stack=rgb_u32), RET 0x4, returns u8
// ---------------------------------------------------------------------------

static mut PALETTE_MAP_COLOR_ADDR: u32 = 0;

/// Call WA's PaletteContext__MapColor to find the nearest display palette index.
#[cfg(target_arch = "x86")]
unsafe fn palette_map_color(palette_ctx: u32, rgb: u32) -> u8 {
    let f: unsafe extern "thiscall" fn(u32, u32) -> u32 =
        core::mem::transmute(PALETTE_MAP_COLOR_ADDR as usize);
    f(palette_ctx, rgb) as u8
}
```

Note: we use `-> u32` for the transmuted function since WA returns the value in EAX (full register), and we mask to u8. This avoids potential ABI issues with `-> u8` on MSVC x86.

- [ ] **Step 2: Add ProcessSprite trampoline and implementation**

Add to `sprite.rs`:

```rust
// ---------------------------------------------------------------------------
// ProcessSprite (0x4FAB80)
// usercall(EAX=sprite, ECX=palette_ctx) + 1 stack(raw_data), RET 0x4
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_process_sprite; impl_fn = process_sprite_impl;
    regs = [eax, ecx]; stack_params = 1; ret_bytes = "0x4");

unsafe extern "cdecl" fn process_sprite_impl(
    sprite: u32,
    palette_ctx: u32,
    raw_data: u32,
) -> u32 {
    use openwa_core::render::spr::parse_spr_header;

    let p = sprite as *mut u8;
    let data_ptr = raw_data as *const u8;

    // We need the data as a slice. Use data_size from header to determine length.
    // data_size is at raw_data + 4.
    let data_size = *(data_ptr.add(4) as *const u32);
    // Total buffer: data_size covers from offset +4 onward, so total = data_size + 4.
    // But we also need the initial 4 unused bytes, so slice length = data_size + 4.
    // However, data_size might not include the bitmap — let's be safe and use
    // data_size + 4 as the slice length (this matches what WA allocated).
    let data_len = (data_size + 4) as usize;
    let data = core::slice::from_raw_parts(data_ptr, data_len);

    let hdr = match parse_spr_header(data) {
        Ok(h) => h,
        Err(e) => {
            // This should never happen with valid WA game data
            panic!("ProcessSprite: failed to parse .spr data: {}", e);
        }
    };

    // --- Update global counter: data_size ---
    let g_data_bytes = rb(va::G_SPRITE_DATA_BYTES) as *mut u32;
    *g_data_bytes = (*g_data_bytes).wrapping_add(data_size);

    // --- Store raw_frame_header_ptr (points to header_flags in raw buffer) ---
    *(p.add(0x60) as *mut *const u8) = data_ptr.add(8);

    // --- Store header_flags ---
    *(p.add(0x14) as *mut u16) = hdr.header_flags;

    // --- Build palette lookup table IN PLACE ---
    // palette_data_ptr points to raw_data + 0x0A (the palette_entry_count field).
    // WA overwrites this region with a 1-indexed palette index lookup table:
    //   [0] = 0 (transparent)
    //   [1..N] = PaletteContext__MapColor(rgb) for each entry
    let palette_base = data_ptr.add(0x0A) as *mut u8;
    *(p.add(0x68) as *mut *mut u8) = palette_base;

    // Update global counter: palette bytes
    let g_palette = rb(va::G_SPRITE_PALETTE_BYTES) as *mut u32;
    *g_palette = (*g_palette).wrapping_add(hdr.palette_count as u32 * 3);

    // Write transparent index
    *palette_base = 0;

    // Map each RGB entry to display palette index
    let rgb_start = data_ptr.add(hdr.palette_offset);
    for i in 0..hdr.palette_count as usize {
        // Read 4 bytes (3 RGB + 1 from next entry, matching WA behavior)
        let rgb_val = *(rgb_start.add(i * 3) as *const u32);
        let mapped = palette_map_color(palette_ctx, rgb_val);
        *palette_base.add(1 + i) = mapped;
    }

    // --- Secondary frame table (if header_flags & 0x4000) ---
    let has_secondary = hdr.header_flags & 0x4000 != 0;
    if has_secondary {
        *(p.add(0x30) as *mut u16) = hdr.secondary_frame_count;
        *(p.add(0x2C) as *mut *const u8) = data_ptr.add(hdr.secondary_frame_offset);
    }

    // --- Main frame header fields ---
    // Copy unknown_08 + fps as a single u32 (matching WA's 4-byte copy)
    let frame_header_ptr = if has_secondary {
        // After secondary frames
        data_ptr.add(hdr.secondary_frame_offset + hdr.secondary_frame_count as usize * 12)
    } else {
        data_ptr.add(hdr.palette_offset + hdr.palette_count as usize * 3)
    };

    // WA copies 4 bytes at once: *(u32*)(sprite+8) = *(u32*)(frame_header)
    *(p.add(0x08) as *mut u32) = *(frame_header_ptr as *const u32);
    *(p.add(0x10) as *mut u16) = hdr.flags;
    *(p.add(0x0C) as *mut u16) = hdr.width;
    *(p.add(0x0E) as *mut u16) = hdr.height;
    *(p.add(0x12) as *mut u16) = hdr.frame_count;
    *(p.add(0x16) as *mut u16) = hdr.max_frames;

    // --- Scale fields ---
    if hdr.is_scaled {
        *(p.add(0x1C) as *mut u32) = hdr.scale_x;
        *(p.add(0x20) as *mut u32) = hdr.scale_y;
        *(p.add(0x24) as *mut u32) = 1; // is_scaled
    } else {
        *(p.add(0x24) as *mut u32) = 0;
    }

    // --- Frame metadata and bitmap pointers ---
    let frame_meta_ptr = data_ptr.add(hdr.frame_meta_offset);
    let bitmap_ptr = data_ptr.add(hdr.bitmap_offset);
    *(p.add(0x28) as *mut *const u8) = frame_meta_ptr;
    *(p.add(0x64) as *mut *const u8) = bitmap_ptr;

    // --- Bitmap palette remapping (only when NO secondary frames) ---
    if !has_secondary {
        // Remap every bitmap byte: pixel = lookup_table[pixel]
        // WA processes (data_size - bitmap_offset_relative + 3) / 4 dwords = that many * 4 bytes
        // We compute the same count for exact WA behavior.
        let bitmap_start_relative = hdr.bitmap_offset;
        let bitmap_byte_count_raw = (data_size as usize + 4).saturating_sub(bitmap_start_relative);
        let dword_count = (bitmap_byte_count_raw + 3) / 4;
        let remap_byte_count = dword_count * 4;

        let bmp = bitmap_ptr as *mut u8;
        for i in 0..remap_byte_count {
            let idx = *bmp.add(i) as usize;
            *bmp.add(i) = *palette_base.add(idx);
        }
    }

    // --- Update global counters: pixel area and frame count ---
    if hdr.frame_count > 0 {
        let g_pixel_area = rb(va::G_SPRITE_PIXEL_AREA) as *mut u32;
        let frames_ptr = frame_meta_ptr as *const [u8; 12];
        for i in 0..hdr.frame_count as usize {
            let frame = &*frames_ptr.add(i);
            let start_x = i16::from_le_bytes([frame[4], frame[5]]);
            let start_y = i16::from_le_bytes([frame[6], frame[7]]);
            let end_x = i16::from_le_bytes([frame[8], frame[9]]);
            let end_y = i16::from_le_bytes([frame[10], frame[11]]);
            let area = (end_x as i32 - start_x as i32) * (end_y as i32 - start_y as i32);
            *g_pixel_area = (*g_pixel_area).wrapping_add(area as u32);
        }
    }

    let g_frame_count = rb(va::G_SPRITE_FRAME_COUNT) as *mut u32;
    *g_frame_count = (*g_frame_count).wrapping_add(hdr.frame_count as u32);

    1 // success
}
```

- [ ] **Step 3: Update install() to include ProcessSprite hook**

Update the `install()` function:

```rust
pub fn install() -> Result<(), String> {
    unsafe {
        PALETTE_MAP_COLOR_ADDR = rb(va::PALETTE_CONTEXT_MAP_COLOR);

        let _ = hook::install(
            "ConstructSprite",
            va::CONSTRUCT_SPRITE,
            trampoline_construct_sprite as *const (),
        )?;

        let _ = hook::install(
            "ProcessSprite",
            va::PROCESS_SPRITE,
            trampoline_process_sprite as *const (),
        )?;
    }
    Ok(())
}
```

- [ ] **Step 4: Build and verify**

Run: `cargo build -p openwa-dll --release 2>&1 | tail -5`
Expected: Compiles without errors.

- [ ] **Step 5: Commit**

```bash
git add crates/openwa-dll/src/replacements/sprite.rs
git commit -m "feat: add ProcessSprite hook replacement with palette remapping"
```

---

### Task 6: Run Replay Tests

**Files:** None (validation only)

- [ ] **Step 1: Run all replay tests**

Run: `powershell -ExecutionPolicy Bypass -File run-tests.ps1`
Expected: All tests pass. If any fail, the sprite loading replacement has a bug.

- [ ] **Step 2: If tests fail, diagnose**

If a test shows `FAIL` (log mismatch), the sprite loading diverges from WA. Common causes:
- Palette remapping producing different indices (check PaletteContext__MapColor bridge)
- Global counter update order or values wrong
- Bitmap remap byte count off by a few bytes
- Alignment computation wrong for frame metadata offset

If a test shows `CRASH`, check for stack corruption (wrong calling convention).

Use `trace-desync` to find the exact divergent frame if needed:
```
powershell -ExecutionPolicy Bypass -File trace-desync.ps1 testdata/replays/<failing_replay>.WAgame
```

- [ ] **Step 3: Commit if all pass (final)**

```bash
git add -A
git commit -m "feat: sprite parser and hook replacements for ConstructSprite + ProcessSprite"
```

---

## Known Risks

1. **Palette remapping byte count**: WA computes `(data_size - bitmap_relative_offset + 3) / 4` dwords, which may process up to 3 bytes past the actual bitmap data. The implementation matches this behavior to avoid any divergence.

2. **ECX = palette context**: ProcessSprite receives the palette context implicitly in ECX from LoadSpriteFromVfs. The `usercall_trampoline` with `regs = [eax, ecx]` captures both registers correctly.

3. **Reading 4 bytes for RGB**: PaletteContext__MapColor receives a u32 where only the low 3 bytes are RGB. WA reads 4 bytes at each 3-byte-strided position (the 4th byte is garbage from the next entry or padding). This is safe because the function only uses `& 0xFFFFFF`.

4. **Frame header location**: The main frame header's position depends on whether secondary frames exist. The code computes this position from `parse_spr_header`'s offsets, but the WA field copy (`*(u32*)(sprite+8) = *(u32*)(frame_header)`) needs the raw pointer to the frame header in the data buffer. This is computed by walking past palette + optional secondary frames.
