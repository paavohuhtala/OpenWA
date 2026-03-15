# DDGame Constructor Port — Design Spec

## Context

DDGame (0x98B8 bytes) is the central game state object. Its constructor at
0x56E220 is one of the largest remaining WA.exe dependencies — it allocates
and initializes all gameplay subsystems (sprites, render queue, weapon table,
coordinate lists, physics params, etc.).

The init chain is already Rust up to this point:
- `GameEngine__InitHardware` (hardware_init.rs) — creates display, sound, keyboard, palette
- `DDGameWrapper__Constructor` (game_session.rs) — creates the wrapper, calls DDGame ctor via bridge

The DDGame constructor is the last major bridge in the init path. Porting it
to Rust means the entire game object creation is under our control.

## Goal

Replace `DDGame__Constructor` (0x56E220, stdcall 9 params + implicit ECX)
with a Rust `DDGame::new()` that creates the object field by field. Use an
incremental approach: port trivial inits immediately, bridge sub-constructor
calls initially, and eliminate bridges one at a time.

## Approach: Incremental Field-by-Field

### Phase 1 — Decompile & Map

1. Decompile DDGame__Constructor (0x56E220) fully in Ghidra
2. Map every field write to our DDGame struct (already partially mapped in ddgame.rs)
3. Categorize each initialization:
   - **Trivial**: zero-fill, constant, pointer copy from params → port immediately
   - **Sub-call**: allocates/inits a sub-object → bridge initially
   - **Complex**: loops, conditionals, computed values → understand then port
4. Add any missing struct fields discovered during decompilation
5. Label all sub-functions called by the constructor in Ghidra

### Phase 2 — Rust DDGame::new()

1. Create `DDGame::new(params...) -> *mut DDGame` in openwa-core
   - Allocates 0x98B8 bytes via `wa_malloc` (or `WABox<DDGame>`)
   - Performs all trivial field initializations in Rust
   - Calls WA sub-functions via typed bridge functions for complex parts
2. Replace the `ddgame_constructor_call` naked bridge in game_session.rs
   with a direct call to `DDGame::new()`
3. Trap the original DDGame__Constructor (it should no longer be called)
4. Verify with replay tests

### Phase 3 — Bridge Elimination (iterative)

Convert each bridged sub-call to pure Rust, simplest first:

| Priority | Sub-call | Description |
|----------|----------|-------------|
| 1 | Field zero-fill regions | memset-style clears of large unknown regions |
| 2 | Sprite cache init | Pointer array zeroing / allocation |
| 3 | Coordinate list init | FUN_004FB3A0 — alloc + init capacity |
| 4 | Object pool init | DDGame+0x600..0x3600 permutation arrays |
| 5 | Render queue setup | Alloc + init the RenderQueue object |
| 6 | Weapon table init | Alloc + populate weapon data tables |
| 7 | Game state init | Complex conditional logic, terrain hooks |

Each bridge elimination is a self-contained commit: convert, test, move on.

## Parameters

The constructor signature (from Ghidra + CLAUDE.md):
```
DDGame__Constructor(
    this,           // allocated 0x98B8 bytes
    keyboard,       // *mut DDKeyboard
    display,        // *mut DDDisplay
    sound,          // *mut DSSound (or null)
    palette,        // *mut Palette
    music,          // *mut Music (or null)
    param7,         // unknown (0x1F4 observed)
    caller,         // parent/caller pointer (often null)
    game_info       // *mut GameInfo
)
// Implicit: ECX = network pointer (DDNetGameWrapper or null)
// stdcall, RET 0x24 (9 stack params = 36 bytes)
```

## Critical Files

| File | Action |
|------|--------|
| `crates/openwa-core/src/engine/ddgame.rs` | Add `DDGame::new()`, fill unknown fields |
| `crates/openwa-wormkit/src/replacements/game_session.rs` | Replace bridge with `DDGame::new()` |
| `crates/openwa-core/src/address.rs` | Add addresses for sub-functions |

## Verification

Each phase is verified independently:
- **Headless replay test**: determinism preserved (byte-identical log output)
- **Headful replay test**: game plays, no crashes, validation passes
- **Manual gameplay**: start match, play a turn, verify no visual/audio glitches
- **Struct assertions**: `size_of::<DDGame>() == 0x98B8` still holds

## Risks

- **Unknown fields**: DDGame has large unmapped regions (0x550..0x2600, 0x2E00..0x45EC, etc.). The constructor may write to fields we haven't identified. Mitigation: zero-fill unknowns and compare behavior.
- **Sub-call ordering**: Some sub-calls may depend on fields set by earlier sub-calls. Mitigation: preserve exact ordering from the decompile.
- **Implicit state**: ECX (network pointer) and global state may affect initialization. Mitigation: capture all implicit params in the bridge.
