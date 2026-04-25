# Engine Initialization Chain

## Overview

WA.exe initializes the engine through a chain of constructor calls that build
the DDGame monolith and its subsystems. The top-level entry is
`ConstructDDGameWrapper` which allocates DDGame and wires up all subsystems.

## Call Chain

```
WinMain / CRT startup (0x5D8B6C)
  └─ ConstructDDGameWrapper (0x56DEF0)
       ├─ Allocates DDGameWrapper (vtable 0x66A30C)
       ├─ DDGame__Constructor (0x56E220)
       │    ├─ Allocates 0x98B8 bytes for DDGame
       │    ├─ DDDisplay__Init (0x569D00)
       │    │    └─ Sets display mode, dimensions, palette, HWND
       │    ├─ GfxHandler x2 (vtable 0x66B280, 0x19C bytes each)
       │    ├─ Task state machines x5 (vtable 0x664118)
       │    └─ Game state pointer at DDGame+0x24
       ├─ Landscape__Constructor (0x57ACB0)
       │    ├─ vtable 0x66B208, parent = DDGame
       │    ├─ Allocates terrain buffer (0x60000 bytes) at +0x8E8
       │    ├─ Creates WaterEffect object (vtable 0x66B268) at +0xC8
       │    ├─ Creates LandscapeShader (vtable 0x66B1DC) at +0x93C
       │    └─ Loads Water.dir, Level.dir from data\Gfx\
       ├─ DSSound__Constructor (0x573D50)
       │    ├─ vtable 0x66AF20
       │    ├─ Master volume = Fixed(0x10000) = 1.0 at +0x8A4
       │    └─ 500 sound channel slots
       └─ OpenGLCPU__Constructor (0x5A0850) [conditional]
            ├─ vtable 0x6774C0, 0x48 bytes
            ├─ Width at +0x10, Height at +0x24
            └─ OpenGL__Init (0x59F000) stores HDC/HGLRC in DDDisplay
```

## DDGameWrapper Layout

DDGameWrapper is a thin wrapper (~0x500 bytes) around DDGame:

| Offset | Field            | Notes                          |
| ------ | ---------------- | ------------------------------ |
| 0x000  | vtable           | 0x66A30C                       |
| 0x488  | DDGame ptr       | Allocated 0x98B8-byte object   |
| 0x48C  | DDGame secondary | Optional 0x2C-byte struct      |
| 0x4C0  | GfxHandler 0     | 0x19C bytes, vtable 0x66B280   |
| 0x4C4  | GfxHandler 1     | Optional                       |
| 0x4C8  | GfxMode          | Graphics mode flag             |
| 0x4CC  | Landscape        | Pointer to landscape subsystem |

## DDGame Key Offsets

DDGame is a ~39KB (0x98B8 bytes) monolithic object. Known landmark fields:

| Offset | Field            | Notes                           |
| ------ | ---------------- | ------------------------------- |
| 0x0024 | game_state       | Global game state pointer       |
| 0x0028 | param            | Constructor parameter           |
| 0x11B0 | task_ptrs[5]     | 5 task state machine pointers   |
| 0x3548 | display_mode     | Display mode pointer            |
| 0x354C | display_width    | Screen width                    |
| 0x3560 | display_center_x | Screen center X                 |
| 0x3564 | display_center_y | Screen center Y                 |
| 0x3578 | hwnd             | Window handle                   |
| 0x358D | palette          | 256-color palette (0x400 bytes) |
| 0x3D98 | gfx_objects[4]   | Graphics object pointers        |

## Ghidra Offset Convention

Ghidra's decompiler shows DWORD-indexed offsets for struct access:

- `param_1[0x122]` → byte offset 0x122 \* 4 = **0x488**
- `param_1[0x133]` → byte offset 0x133 \* 4 = **0x4CC**
- Always multiply DWORD index by 4 to get the true byte offset.
