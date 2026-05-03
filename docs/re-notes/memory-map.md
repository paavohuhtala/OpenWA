# Memory Map - WA.exe 3.8.1

## Segments

| Segment | Start      | End        | Size    | Description             |
| ------- | ---------- | ---------- | ------- | ----------------------- |
| Headers | 0x00400000 | 0x00400FFF | 4 KB    | PE headers              |
| .text   | 0x00401000 | 0x00619FFF | ~2.1 MB | Code                    |
| .rdata  | 0x0061A000 | 0x00693FFF | ~484 KB | Read-only data, vtables |
| .data   | 0x00694000 | 0x008C4157 | ~2.2 MB | Read-write globals      |
| .rsrc   | 0x008C5000 | 0x00954FFF | ~576 KB | Resources               |
| .reloc  | 0x00955000 | 0x00983FFF | ~188 KB | Relocations             |

## Key Statistics

- Total functions identified: 6,859
- Image base: 0x00400000
- 32-bit x86 PE, compiled with MSVC
- Entry point: 0x005D8B6C (CRT startup)
- Export: SetHostingProxyAddressAndPort @ 0x0058E380

## GameWorld structure access

The main game state is a 0x98B8-byte (39KB) monolithic object allocated by
`GameWorld__Constructor` (0x56E220). It is wrapped by GameRuntime (0x56DEF0).

### GameRuntime offsets (from wrapper base)

- +0x488 → GameWorld pointer (the allocated 0x98B8-byte object)
- +0x4C0 → GfxHandler 0 (vtable 0x66B280)
- +0x4CC → Landscape pointer

### GameWorld offsets (from GameWorld base)

- +0x0024 → Game state pointer
- +0x0028 → Constructor param
- +0x11B0 → Entity state machine pointers (5 entries)
- +0x3548 → Display mode
- +0x354C → Display width
- +0x3560 → Display center X
- +0x3564 → Display center Y
- +0x3578 → HWND
- +0x358D → Palette (0x400 bytes)
- +0x3D98 → Gfx object pointers (4 entries)

### WormKit-convention offsets (DWORD-indexed)

These offsets are from the GameRuntime base, as used by WormKit mods:

- GameWorld+0x08 → WorldRoot object
- GameWorld+0x488 → Game global state
- GameWorld+0x4CC → PC_Landscape
- GameWorld+0x510 → Weapon table
- GameWorld+0x548 → Weapon panel

## Subsystem vtables

| Address  | Object           | Size   | Notes                     |
| -------- | ---------------- | ------ | ------------------------- |
| 0x66A30C | GameRuntime      | ~0x500 | Top-level wrapper         |
| 0x66B280 | GfxHandler       | 0x19C  | 2 instances               |
| 0x664144 | DisplayGfx       | —      |                           |
| 0x66B208 | Landscape        | 0xB40  | Terrain, water, level     |
| 0x66B1DC | LandscapeShader  | —      | Used by Landscape         |
| 0x66B268 | WaterEffect      | 0xBC   | Created by Landscape      |
| 0x66AF20 | DSSound          | ~0xBD0 | DirectSound, 500 channels |
| 0x664118 | EntityStateMachine | —      | 5 instances in GameWorld  |
| 0x6774C0 | OpenGLCPU        | 0x48   | Optional, conditional     |

## Fixed-point convention

All coordinates, velocities, and physics values use 16.16 fixed-point:

- `0x10000` = 1.0
- `0x20000` = 2.0
- `0xCCCC` ≈ 0.8
- Positions stored as Fixed at WorldEntity offsets 0x84 (X) and 0x88 (Y)
- Velocities stored as Fixed at WorldEntity offsets 0x90 (X) and 0x94 (Y)
