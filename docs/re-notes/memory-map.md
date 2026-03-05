# Memory Map - WA.exe 3.8.1

## Segments

| Segment | Start      | End        | Size     | Description          |
|---------|------------|------------|----------|----------------------|
| Headers | 0x00400000 | 0x00400FFF | 4 KB     | PE headers           |
| .text   | 0x00401000 | 0x00619FFF | ~2.1 MB  | Code                 |
| .rdata  | 0x0061A000 | 0x00693FFF | ~484 KB  | Read-only data, vtables |
| .data   | 0x00694000 | 0x008C4157 | ~2.2 MB  | Read-write globals   |
| .rsrc   | 0x008C5000 | 0x00954FFF | ~576 KB  | Resources            |
| .reloc  | 0x00955000 | 0x00983FFF | ~188 KB  | Relocations          |

## Key Statistics

- Total functions identified: 6,859
- Image base: 0x00400000
- 32-bit x86 PE, compiled with MSVC
- Entry point: 0x005D8B6C (CRT startup)
- Export: SetHostingProxyAddressAndPort @ 0x0058E380

## DDGame structure access

The main game state is accessed through the DDGame pointer:
- DDGame obtained via `ConstructDDGameWrapper` (0x56DEF0) parameter
- DDGame+0x08  → TurnGame object
- DDGame+0x488 → Game global state
- DDGame+0x4CC → PC_Landscape
- DDGame+0x510 → Weapon table
- DDGame+0x548 → Weapon panel

## Fixed-point convention

All coordinates, velocities, and physics values use 16.16 fixed-point:
- `0x10000` = 1.0
- `0x20000` = 2.0
- `0xCCCC` ≈ 0.8
- Positions stored as Fixed at CGameTask offsets 0x84 (X) and 0x88 (Y)
- Velocities stored as Fixed at CGameTask offsets 0x90 (X) and 0x94 (Y)
