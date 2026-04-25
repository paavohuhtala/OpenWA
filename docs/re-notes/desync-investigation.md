# Desync Investigation Journal: Seven Bugs, One Checksum Mismatch

## Summary

During the incremental Rust reimplementation of Worms Armageddon's GameWorld constructor and gameplay hooks, a deterministic replay of a longbow match produced "Checksum Mismatch" errors. What appeared to be a single constructor bug turned out to be seven independent issues across three subsystems (constructor, replay loader, weapon dispatch), found over multiple debugging sessions using hardware watchpoints, sub-object hashing, binary search on hooks, and targeted test replays.

## Background

OpenWA replaces WA functions with Rust reimplementations via DLL injection. The GameWorld constructor (~900 lines of Rust replacing a ~19KB x86 function) initialises the entire game engine: display layers, terrain, sprites, sound, HUD, and entity systems. An A/B toggle (`OPENWA_USE_ORIG_CTOR=1`) switches between the Rust and original constructors for comparison.

Replay testing works by running WA.exe with a recorded `.WAgame` file in headless mode (no graphics window), capturing the game log, and comparing against a known-good baseline. WA's built-in checksum system sends periodic state checksums during gameplay; mismatches indicate simulation divergence.

## The Symptom

- Rust constructor: **4 checksum mismatches** at frames 1350, 2186, 2249, 2775
- Original constructor: all checksums pass
- GameWorld flat memory (39KB): **byte-for-byte identical** between both constructors
- Terrain collision bitmaps: identical
- Entity tree: same 68 entities in same BFS order

The game outcome was the same (same winner, same damage), but the checksums diverged.

## Dead Ends

Several hypotheses were investigated and ruled out:

1. **Landscape bitmap differences** — dumped 1.3MB terrain collision bitmaps from both runs: byte-identical at construction AND at the divergence frame.

2. **Entity tree ordering** — full entity snapshot comparison showed ~490 diff lines. Seemed like proof of different entity ordering. But the same comparison at a _known-good_ frame also showed ~490 diffs, and the bots replay (which passes) showed ~480 diffs. The snapshot tool's pointer canonicalization heuristic (`is_likely_pointer` using `can_read()`) was producing false positives. **Lesson: always validate your diff tool against a known baseline.**

3. **PaletteContext buffer** — the original constructor creates a PaletteContext on the stack and reuses it; our code creates a fresh one. Tried matching the initialization: no effect. The cached GfxDir path was taken, bypassing the PaletteContext entirely.

4. **Landscape constructor parameters** — verified via x86 disassembly that `param_5 = game_info + 0xDAAC` (not raw game_info). Our code was already correct.

## The Breakthrough: Sub-Object Diffing

The key insight: GameWorld's flat memory matching doesn't mean _everything_ matches. The constructor calls WA functions that modify objects _outside_ GameWorld — particularly the **display object** (GameRuntime+0x4D0).

Dumping the display object (16KB) before and after construction revealed:

| Offset  | Before | After (orig) | After (rust) |
| ------- | ------ | ------------ | ------------ |
| +0x09C0 | 0      | **3**        | 0            |
| +0x353C | 0      | **1**        | 0            |

The display started identical and diverged _during_ the constructor.

## Hardware Watchpoints With Stack Traces

To find what wrote these display fields, we used x86 debug registers (DR0–DR3) via a Vectored Exception Handler:

1. Register VEH before the constructor
2. Set DR0/DR1 to watch `display + 0x09C0` and `display + 0x353C`
3. On `STATUS_SINGLE_STEP` (write trap), log the value, EIP, and walk the EBP chain for a stack trace

The watchpoint reported:

```
display+0x353C = 0x00000001  eip=0x005234C0  stack=[...]
display+0x09C0 = 0x00000003  eip=0x005234B8  stack=[...]
```

Both writes came from `LoadSprite` (0x523400) — a display vtable method (slot 31) that loads a sprite into the display's layer cache. The sprite being loaded was **index 0x26E** into **layer 3**.

## The Root Cause

Tracing back to our Rust constructor:

```rust
if (*runtime).gfx_mode != 0 {
    DDDisplay::load_sprite_by_layer(disp, 3, 0x26D, land_layer, "back.spr");
    DDDisplay::load_sprite(disp, 3, 0x26E, 0, land_layer, "debris.spr");
}
```

In headless mode (`gfx_mode = 0`), both sprites were skipped. But the original WA constructor loads them unconditionally — even without a graphics window. Sprite 0x26E is `debris.spr`, used by `GenerateDebrisParticles` (0x546F70) for particle effects.

`GenerateDebrisParticles` reads terrain pixels and, when a hit is detected, updates the **game RNG** (GameWorld+0x45EC) and creates particle entities. Without `debris.spr` loaded in the display, the sprite lookup returns different data, causing different RNG update patterns:

- Original: 15,470 RNG writes (from frame 808 onward)
- Rust (broken): 17,106 RNG writes
- Delta: 1,636 extra RNG ticks → cascading simulation divergence

## The Partial Fix (and a Lesson in Verifying Results)

```rust
// Load unconditionally — debris.spr affects game RNG via GenerateDebrisParticles
DDDisplay::load_sprite(disp, 3, 0x26E, 0, land_layer, c"debris.spr".as_ptr());
```

This change is correct — the original constructor loads `debris.spr` unconditionally, and our code should match. However, it did **not** resolve the desync. An initial test appeared to show 3 of 4 mismatches fixed, but this was a `tail -5` truncation error — only the last mismatch was visible, creating a false impression. All 4 mismatches persist.

The display state differences (sprite layer counters) we found were real and worth fixing for correctness, but they turned out not to be the root cause of the checksum mismatch.

---

## Phase 2: RNG Tracing and the Debris Particle Divergence

### Per-frame RNG tracking

With the sprite fix applied but the desync persisting, the next step was tracking the game RNG (GameWorld+0x45EC) at frame boundaries. A hook on `TurnManager_ProcessFrame` logged the RNG value before and after each frame:

```
game_f=817  rng=B714AF97  (identical in both runs)
game_f=818  rng=BE0D887F  (Rust) vs ADB76793 (orig)
```

The RNG was **identical through frame 817** and diverged during frame 818. But `AdvanceGameRNG` (0x53F320) turned out to be **mostly inlined** — hooking the function only caught rare non-inlined calls. The real tool was a hardware watchpoint on GameWorld+0x45EC itself.

### Hardware watchpoint on the RNG field

Arming DR0 on GameWorld+0x45EC at frame 817 captured every RNG write with its EIP. Comparing the two runs:

| Write # | Rust EIP              | Orig EIP                    | Notes                                       |
| ------- | --------------------- | --------------------------- | ------------------------------------------- |
| 1–24    | 0x5470BB/DE           | 0x5470BB/DE                 | Identical (GenerateDebrisParticles inlined) |
| 25      | **0x6529960D** (hook) | **0x5470BB** (more debris!) | Divergence point                            |

The original had **36 debris writes** (3 × GenerateDebrisParticles calls) while Rust had only **24** (2 calls). One terrain collision event was missing. Yet total debris writes across the game were identical (72 each) — just distributed differently across frames.

### Caller frequency analysis

Counting all 17K+ RNG write EIPs revealed the dominant divergence source: `AdvanceGameRNG_Low16` (0x507CE0) in `FireEntity__HandleMessage` — 14,085 calls (Rust) vs 12,517 (orig). The 1,568 extra calls represented projectiles processing more frames due to the cascading RNG shift.

## Phase 3: The Flat Memory Puzzle

At this point, GameWorld flat memory was verified **byte-for-byte identical** between constructors. Display, landscape, and wrapper non-pointer values all matched at frame 0. The desync was deterministic. What could differ?

### Sub-object hashing: the breakthrough

The key insight: GameWorld contains **pointers** to heap-allocated sub-objects. Flat memory comparison only shows the pointer values (which differ between runs due to heap layout), not the _content_ of what they point to.

A new tool — `hash_pointer_targets()` — walks every DWORD in GameWorld, follows each heap pointer, and hashes the first 256 bytes of the target with pointer canonicalization (replacing pointer-looking values with 0).

The first run produced hundreds of hash differences due to an overly aggressive pointer filter. After tuning to use the existing `is_likely_pointer` heuristic, a targeted raw dump of specific sub-objects revealed the smoking gun:

**Arrow collision region** (SpriteRegion at GameWorld+0x48C):

| Offset          | Rust   | Original |
| --------------- | ------ | -------- |
| +0x14 (this[5]) | **0**  | **10**   |
| +0x18 (this[6]) | **20** | **10**   |

Different collision box dimensions. The Rust constructor was creating origin-based collision boxes instead of centered boxes with 10px margins.

### The root cause: wrong SpriteRegion parameters

Tracing the SpriteRegion constructor (0x57DB20) parameters via disassembly:

```
this[3] = p4 - p2    (width)
this[4] = EDX - p3   (height)
this[5] = ECX - p2   (x inset)
this[6] = p5 - p3    (y inset)
```

The original computes a **centered** collision box:

- ECX = `sprite_w / 2`, EDX = `sprite_h - margin_h`
- p2 = `margin_w`, p3 = `margin_h` (where margin = max(0, dim/2 − 10))
- p4 = `sprite_w - margin_w`, p5 = `sprite_h / 2`
- p6 = **arrow sprite pointer**

Our code passed:

- ECX = **0**, EDX = `margin_h`
- p2 = **0**, p3 = **0**
- p4 = `margin_w`, p5 = `margin_h`
- p6 = **landscape gfx_resource** (wrong object entirely — from an outer scope)

Six of eight parameters were wrong. The collision boxes were origin-based instead of centered, AND referenced the wrong sprite, causing different terrain collision detection for arrow projectiles.

## Phase 4: It Wasn't Just the Constructor

### New test replays change everything

Recording two additional replays proved crucial:

- **longbow_water** — longbow fired at water (no terrain collision)
- **longbow_alt_theme** — longbow on a different WA theme

Expected logs were generated from **unmodded WA.exe** (critical — our DLL's hooks can affect the game log).

Results with the original constructor:

| Replay            | Status              |
| ----------------- | ------------------- |
| longbow_water     | **FAIL**            |
| longbow_alt_theme | **FAIL** (Frame 0!) |

Both failed with the _original_ constructor! The desync wasn't constructor-specific — it was caused by **gameplay hooks**.

### Binary search on hooks

Disabling hooks in groups narrowed the search:

1. All hooks disabled except essentials → PASS
2. Add weapon hooks → FAIL
3. Only weapon hooks, disable FireWeapon → PASS for longbow_water
4. Only replay hooks → FAIL for alt_theme

Two independent hook bugs:

**ReplayLoader (replay.rs):**

- Player array strides: Ghidra showed element indices (`i*0x3C` for `u16*`, `i*0x1E` for `u32*`) but code used them as byte offsets. Correct byte stride: `i*0x78`.
- Non-zero `map_seed` path: per-team weapon config reads from stream were unimplemented. Fallback to original parser for `map_seed ≠ 0`.

**FireWeapon (weapon.rs):**

- Binary search within the type-4 dispatch: type 4 → subtypes 10+13 → subtype 13 (Napalm Strike).
- `fire_send_team_message` wrote `team_index` to `buf[8..12]` but the original writes it to `buf[0..4]`. The HandleMessage handler reads `data[0]`, receiving 0 instead of the correct team index.
- Root cause confirmed by disassembly of the original Napalm handler at 0x51E5C0: `MOV [ESP+4], EAX` where EAX = `worm+0xFC` (team_index) and `ESP+4` = `buffer[0]`.

### Also fixed along the way

- **GameWorld allocation size**: 0x98B8 → 0x98D8 (original mallocs 0x98D8 but memsets only 0x98B8)
- **Wrong function after InitPaletteGradientSprites**: called `LoadingProgressTick` (0x5717A0) instead of `DisplayGfx__InitTeamPaletteDisplayObjects` (0x5703E0)

## Debugging Tools Built

The investigation produced several reusable tools:

- **Hardware watchpoint system** (`debug_watchpoint.rs`) — x86 DR0–DR3 with VEH handler and EBP stack trace walking
- **Sub-object hashing** (`snapshot.rs: hash_pointer_targets`) — follows heap pointers in a struct, hashes target content with pointer canonicalization. Integrated into the snapshot system.
- **Per-frame RNG logging** via TurnManager hook
- **`OPENWA_WATCH_DISPLAY=1`** / **`OPENWA_WATCH_FRAME=N`** — arm watchpoints on display or GameWorld at specific frames
- **Binary sub-object dumping** for display, GfxHandler, Landscape comparison
- **A/B constructor toggle** (`OPENWA_USE_ORIG_CTOR=1`) — instant switching between Rust and original constructors

## Key Lessons

1. **Identical struct memory ≠ identical behaviour.** GameWorld flat memory was byte-identical, but _sub-objects pointed to by GameWorld_ had different content. The `hash_pointer_targets` tool was purpose-built to catch this.

2. **Validate your diff tools.** Our snapshot comparison showed ~490 "differences" at known-good frames — pure noise from pointer canonicalization heuristics. And our dump code once read past a heap allocation into adjacent memory, producing false diffs. Always baseline.

3. **There is no "visual-only" code in a deterministic game engine.** `debris.spr` seems purely decorative, but Worms uses a **single shared RNG** for everything. Skipping one sprite load shifts the entire RNG sequence.

4. **Hardware watchpoints are underrated.** x86 DR0–DR3 with a VEH handler gives "what wrote this byte?" answers in seconds, without an external debugger.

5. **Binary search on frames, not code.** Per-frame RNG logging narrowed 542 candidate frames to the exact frame (818) where divergence began.

6. **Binary search on hooks.** Disabling half the hooks at a time isolated two independent bugs in minutes. Then within a single hook (FireWeapon), binary search on dispatch paths (type → subtype → specific handler) pinpointed the exact buggy function.

7. **Multiple test replays expose different bugs.** The original longbow replay only triggered the constructor bug. Adding longbow_water and longbow_alt_theme exposed hook bugs that were invisible with the original replay alone. Different weapons, themes, and targets exercise different code paths.

8. **Ghidra element indexing is a trap.** The decompiler shows `(&DAT[i*0x3C])` for a `u16*` array — the `0x3C` is an _element_ index, not a byte offset. The actual byte offset is `i * 0x3C * sizeof(u16) = i * 0x78`. This caused player data corruption for index 1+.

9. **Check the buffer layout against disassembly.** The Napalm Strike bug was a single wrong offset in a message buffer: `buf[8]` vs `buf[0]`. The original's disassembly (`MOV [ESP+4], EAX` at 0x51E5D2) unambiguously shows the write destination. When in doubt, read the assembly.
