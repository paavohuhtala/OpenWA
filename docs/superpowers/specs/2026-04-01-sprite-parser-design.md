# Sprite Parser Design

Port WA's `.spr` sprite loading to a clean Rust parser with WormKit hook replacements.

## Goal

1. A pure Rust `.spr` parser (`ParsedSprite`) in `openwa-core` — no WA dependencies, usable in tests/tooling
2. Hook replacements for `ConstructSprite` (0x4FAA30) and `ProcessSprite` (0x4FAB80) — WA calls our code instead of its own

## .spr File Format

Binary format parsed by ProcessSprite (0x4FAB80):

```
Offset  Size     Field
+0x00   4        (unused / magic)
+0x04   4 (u32)  data_size — accumulated in global counter at 0x7A0864
+0x08   2 (u16)  header_flags → sprite+0x14
+0x0A   2 (u16)  palette_entry_count
+0x0C   3 * N    palette entries (3 bytes each, RGB)
...     varies   frame metadata (SpriteFrame, 0x0C bytes each)
...     varies   bitmap pixel data (8-bit indexed)
```

Special flags:
- **0x4000** in header_flags: secondary frame table present before primary frames.
  First word after palette = secondary frame count, followed by 0x0C-byte entries.
- **Negative frame count** (bit 15 set): scaling mode.
  High 7 bits = scale_x, low 7 bits = scale_y. Actual frame_count set to 1.
  Scale values stored as `(raw_value << 16) >> 5`.

## ParsedSprite Type

New file `crates/openwa-core/src/render/spr.rs`:

```rust
pub enum SprError {
    TooShort,
    InvalidData(&'static str),
}

pub struct ParsedSprite {
    pub fps: u16,
    pub width: u16,
    pub height: u16,
    pub flags: u16,
    pub header_flags: u16,
    pub frame_count: u16,
    pub max_frames: u16,
    pub scale_x: u32,           // raw 7-bit value; 0 if not scaled
    pub scale_y: u32,           // raw 7-bit value; 0 if not scaled
    pub is_scaled: bool,
    pub frames: Vec<SpriteFrame>,
    pub secondary_frames: Vec<SpriteFrame>,  // non-empty iff header_flags & 0x4000
    pub bitmap: Vec<u8>,        // 8-bit indexed pixels, same layout as WA
    pub palette: Vec<[u8; 3]>,  // RGB triples
    pub data_size: u32,         // from .spr +0x04, for global counter updates
}
```

`ParsedSprite::parse(data: &[u8]) -> Result<Self, SprError>` — pure function, no WA dependencies, no `#[cfg(target_arch)]`.

## Hook Adapter

In `spr.rs`, gated by `#[cfg(target_arch = "x86")]`:

```rust
pub unsafe fn populate_wa_sprite(sprite: *mut Sprite, parsed: &ParsedSprite) { ... }
```

- Copies scalar fields (fps, width, height, flags, etc.) into the WA Sprite struct
- `wa_malloc` + `memcpy` for frame metadata, bitmap data, palette data
- Applies scale transform: `(raw_value << 16) >> 5` for scale_x/scale_y
- Updates 4 global counters: `G_SPRITE_DATA_BYTES`, `G_SPRITE_FRAME_COUNT`, `G_SPRITE_PIXEL_AREA`, `G_SPRITE_PALETTE_BYTES`

Pixel data is copied verbatim — no format conversion needed.

## Hook Replacements

New file `crates/openwa-wormkit/src/replacements/sprite.rs`:

### ConstructSprite (0x4FAA30)

Convention: `__usercall` — EAX=sprite, ECX=context.

Replacement:
- Set vtable to `rb(0x66418C)`
- Set DisplayGfx vtable at +0x34 to `rb(0x664144)`
- Zero all other fields

Trampoline: `usercall_trampoline!(regs = [eax, ecx])`.

### ProcessSprite (0x4FAB80)

Convention: `__usercall` — EAX=sprite, 1 stack param (raw data pointer).

Replacement:
1. Determine raw data extent (bitmap data extends to end of allocation — length comes from context)
2. Call `ParsedSprite::parse(data)`
3. Call `populate_wa_sprite(sprite, &parsed)`
4. On parse error: log and panic (corrupt game data, should never happen)

Trampoline: `usercall_trampoline!(reg = eax; stack_params = 1; ret_bytes = "0x4")`.

## Open Question: Data Length

ProcessSprite receives a raw data pointer but no explicit length. The caller (`LoadSpriteFromVfs`) allocates the buffer and knows its size. Options:
- Parse defensively using `data_size` from the header (+0x04) to bound reads
- Hook `LoadSpriteFromVfs` to pass length through (out of scope for now)
- The parser can derive bounds from internal fields (palette count, frame count, bitmap offsets)

Recommended: use internal fields to derive bounds. The `data_size` header field gives total size. Validate reads against it.

## File Changes

| File | Change |
|---|---|
| `crates/openwa-core/src/render/spr.rs` | New: `ParsedSprite`, `SprError`, `parse()`, `populate_wa_sprite()` |
| `crates/openwa-core/src/render/mod.rs` | Add `pub mod spr;` |
| `crates/openwa-wormkit/src/replacements/sprite.rs` | New: hook trampolines + `install()` |
| `crates/openwa-wormkit/src/replacements/mod.rs` | Wire `sprite::install()` |

## Verification

- **Unit tests** in `openwa-core`: synthetic `.spr` payloads (single frame, multi-frame, scaled, secondary frames)
- **Replay tests**: `.\run-tests.ps1` — all existing replays must pass byte-for-byte (global counters and sprite state must match exactly)
- If replays show desyncs, use `trace-desync` to identify the divergent frame
