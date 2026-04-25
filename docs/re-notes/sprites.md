# Sprite System

WA.exe uses a custom `.spr` format for paletted animated sprites. The DD_Display object
manages a 1024-slot sprite table, loading sprites from `.dir` archive files via the
GfxDir subsystem.

## DD_Display Sprite Storage

DD_Display contains four arrays for sprite management:

| Byte Offset | Size | Description |
|---|---|---|
| +0x0008 | 4096 (1024 × u32) | `sprite_palette[1024]` — palette ID (1-3) per slot |
| +0x1008 | 4096 (1024 × Ptr32) | `sprite_ptrs[1024]` — Sprite object pointer per slot |
| +0x311C | 12 (3 × Ptr32) | `gfx_dirs[3]` — GfxDir pointer per palette |
| +0x3530 | 12 (3 × u32) | `sprite_counts[3]` — loaded sprite count per palette |

Sprite IDs range from 1 to 1023. Bit 0x800000 on the ID signals "already loaded" and
causes LoadSprite to return immediately with success.

## Sprite Object (0x70 bytes)

Vtable: `0x66418C`. Created by ConstructSprite (0x4FAA30), populated by ProcessSprite
(0x4FAB80).

| Offset | Size | Type | Name | Source |
|---|---|---|---|---|
| 0x00 | 4 | Ptr32 | vtable | ConstructSprite |
| 0x04 | 4 | Ptr32 | context_ptr | ConstructSprite (ECX param) |
| 0x08 | 2 | u16 | _unknown_08 | wkJellyWorm |
| 0x0A | 2 | u16 | fps | wkJellyWorm |
| 0x0C | 2 | u16 | width | wkJellyWorm, ProcessSprite |
| 0x0E | 2 | u16 | height | wkJellyWorm, ProcessSprite |
| 0x10 | 2 | u16 | flags | wkJellyWorm, ProcessSprite |
| 0x12 | 2 | u16 | frame_count | ProcessSprite (can be overwritten) |
| 0x14 | 2 | u16 | header_flags | ProcessSprite (from .spr raw+8) |
| 0x16 | 2 | u16 | max_frames | wkJellyWorm, ProcessSprite |
| 0x18 | 2 | u16 | _unknown_18 | |
| 0x1A | 2 | u16 | _unknown_1a | |
| 0x1C | 4 | u32 | scale_x | ProcessSprite (negative frame count) |
| 0x20 | 4 | u32 | scale_y | ProcessSprite (negative frame count) |
| 0x24 | 4 | u32 | is_scaled | ProcessSprite (1 if scaled, 0 otherwise) |
| 0x28 | 4 | Ptr32 | frame_meta_ptr | SpriteFrame* array |
| 0x2C | 4 | Ptr32 | secondary_frame_ptr | 0x4000 flag in header_flags |
| 0x30 | 2 | u16 | secondary_frame_count | |
| 0x32 | 2 | u16 | _pad_32 | |
| 0x34 | 4 | Ptr32 | display_gfx | DisplayGfx vtable (0x664144) |
| 0x38 | 40 | [u8; 0x28] | _unknown_38 | |
| 0x60 | 4 | Ptr32 | raw_frame_header_ptr | ProcessSprite |
| 0x64 | 4 | Ptr32 | bitmap_data_ptr | ProcessSprite |
| 0x68 | 4 | Ptr32 | palette_data_ptr | ProcessSprite |
| 0x6C | 4 | u32 | _unknown_6c | |

### SpriteFrame (0x0C bytes)

Per-frame metadata, pointed to by `Sprite::frame_meta_ptr`. One entry per animation frame.

| Offset | Size | Type | Name |
|---|---|---|---|
| 0x00 | 4 | u32 | bitmap_offset |
| 0x04 | 2 | u16 | start_x |
| 0x06 | 2 | u16 | start_y |
| 0x08 | 2 | u16 | end_x |
| 0x0A | 2 | u16 | end_y |

Source: wkJellyWorm `Sprites.h::SpriteFrame`.

## Sprite Vtable (0x66418C, 8 entries)

| Slot | Address | Name |
|---|---|---|
| 0 | 0x4FAA80 | Destructor |
| 1 | 0x4FAAD0 | Unknown |
| 2 | 0x4FB5C0 | Unknown |
| 3 | 0x4FE550 | Unknown |
| 4 | 0x4FE2F0 | Unknown |
| 5 | 0x4FE9C0 | Unknown |
| 6 | 0x5613D0 | BaseEntity common stub |
| 7 | 0x5613D0 | BaseEntity common stub |

## Key Functions

### LoadSprite (0x523400)

`__thiscall` on DD_Display, `RET 0x14` (5 stack params). Loads a sprite into the
DD_Display sprite table.

Parameters:
1. `palette` (1-3) — which GfxDir palette to use
2. `sprite_id` (1-1023) — slot index
3. `size_override` — packed DWORD: low16 = width override, high16 = height override (0 = no override)
4. `file_archive` — PC_FileArchive*
5. `filename` — const char* sprite filename

Returns: 1 on success, 0 on failure.

Flow:
1. If `sprite_id & 0x800000`, return 1 (already loaded)
2. Validate `palette` in [1,3] and `gfx_dirs[palette]` is non-null
3. Validate `sprite_id` in [1,1023]
4. Call vtable[0x20] to check if sprite already exists at this slot
5. Allocate sprite memory via `WA_MallocMemset` (0x53E910)
6. Call `ConstructSprite` (0x4FAA30) to initialize
7. Call `LoadSpriteFromVfs` (0x4FAAF0) to load data
8. Store sprite pointer at `this[sprite_id + 0x402]` (byte offset +0x1008)
9. Store palette at `this[sprite_id + 2]` (byte offset +0x8)
10. Increment `sprite_counts[palette]`
11. Apply `size_override` to sprite +0x16 (width) and +0x18 (height) if non-zero

### ConstructSprite (0x4FAA30)

`__usercall` — EAX = sprite pointer, ECX = context pointer. Initializes a freshly
allocated 0x70-byte sprite struct: sets vtable to 0x66418C, DisplayGfx vtable at +0x34,
zeroes all data fields.

### LoadSpriteFromVfs (0x4FAAF0)

`__usercall` — EAX = filename string, ECX = PC_FileArchive pointer, 2 stack params
(sprite pointer, GfxDir pointer). Tries `GfxDir::FindEntry` (0x566520) for a cached
entry first; on miss, calls `GfxDir::LoadImage` (0x5666D0) to read from the archive.
Allocates a buffer for raw sprite data, then calls `ProcessSprite` (0x4FAB80).

### ProcessSprite (0x4FAB80)

`__usercall` — EAX = sprite pointer, 1 stack param (raw data pointer from .spr file).
Parses the .spr binary format and populates the Sprite struct fields. See the
.spr File Format section below.

### DrawSpriteGlobal (0x541FE0)

`__thiscall` + EAX = y_pos (usercall). 4 stack params: layer, x_pos, sprite_id, frame.
Enqueues a draw command (type 4) to the rendering queue. World-space coordinates.

### DrawSpriteLocal (0x542060)

Identical to DrawSpriteGlobal but enqueues command type 5. Screen-space coordinates.

### DestroySprite (0x4FAA80)

`__thiscall`, sprite vtable slot 0. Frees sprite resources.

## .spr File Format

Binary format parsed by ProcessSprite (0x4FAB80). Data layout:

```
Offset  Size     Field
+0x00   4        (unused / magic)
+0x04   4 (u32)  data_size — added to global counter at 0x7A0864
+0x08   2 (u16)  header_flags → sprite+0x14
+0x0A   2 (u16)  palette_entry_count
+0x0C   3 × N    palette entries (3 bytes each, likely RGB)
...     varies   frame metadata (SpriteFrame, 0x0C bytes each)
...     varies   bitmap pixel data
```

Special flags:
- **0x4000** in header_flags: secondary frame table present before the primary frames.
  Count at first word after palette, followed by 0x0C-byte entries.
- **Negative frame count** (bit 15 set in the frame count word at the frame header):
  high 7 bits → scale_x, low 7 bits → scale_y. Actual frame_count set to 1.
  Scale values stored at sprite +0x1C and +0x20 as `(value << 16) >> 5`.

## Global Counters

Four u32 globals tracking total sprite resource usage, accumulated by ProcessSprite:

| Address | Description |
|---|---|
| 0x7A0864 | Total sprite data bytes loaded |
| 0x7A0868 | Total sprite frame count |
| 0x7A086C | Total sprite pixel area (sum of frame w×h) |
| 0x7A0870 | Total palette entries × 3 |

## GfxDir System

Sprites are stored in `.dir` archive files loaded by the GfxDir subsystem:

- **GfxHandler::LoadDir** (0x5663E0): Loads a `.dir` archive. Reads `"DIR\x1A"`
  header (0x1A524944), parses 0x1000-byte entries, performs pointer relocation.
- **GfxDir::FindEntry** (0x566520): Looks up a cached entry by name.
- **GfxDir::LoadImage** (0x5666D0): Reads image data from archive, returns through
  vtable with 0x0C-byte descriptor (vtable 0x66A1C0).

## Sources

- Ghidra decompilation of LoadSprite, ConstructSprite, ProcessSprite, DrawSpriteGlobal
- wkJellyWorm `Sprites.h` / `Sprites.cpp` — struct layout, function hooks, calling conventions
- Runtime analysis via OpenWA validator DLL
