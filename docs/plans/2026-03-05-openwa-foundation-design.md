# OpenWA Foundation Design

## Goal

Build the type foundation for an incremental Rust reimplementation of Worms Armageddon 3.8.1 (Steam). Start with Ghidra-assisted reverse engineering and Rust type definitions, later expand to a WormKit module that replaces subsystems.

## Strategy: Hybrid Bootstrap

- **Constants/enums**: Translate immediately from wkJellyWorm's well-documented Constants.h (weapons, messages, sprites, sounds, class types)
- **Struct layouts**: Explore in Ghidra first, write Rust types only for verified fields, pad unknowns

## Architecture

```
OpenWA/                     (Cargo workspace)
  crates/
    openwa-types/           (no_std-compatible type definitions)
      - Enums: ClassType, TaskMessage, Weapon, SoundId, SpriteId
      - Structs: CTask, CGameTask (partial, Ghidra-verified)
      - Primitives: FixedPoint (16.16 fixed-point math)
      - Addresses: Known function/global addresses
    openwa-dll/          (future: WormKit DLL module)
    openwa-render/           (future: modern rendering backend)
```

## Key Decisions

- `#[repr(C)]` and `#[repr(u32)]` for FFI compatibility from day one
- Unknown struct fields as `_unknown_XX: [u8; N]` padding — honest about what we don't know
- Fixed-point newtype rather than raw i32 — prevents unit confusion
- No external dependencies in openwa-types (pure Rust, no_std compatible)

## Sources

- wkJellyWorm: Task hierarchy, enums, vtable layouts, offset documentation
- WormKit: Game state pointers, network protocol, fixed-point conventions
- Ghidra: WA.exe analysis (6,859 functions, image base 0x400000)
