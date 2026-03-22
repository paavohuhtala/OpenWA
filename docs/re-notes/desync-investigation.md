# The debris.spr Desync: Debugging a Replay Mismatch in a Reimplemented Game Constructor

## Summary

During the incremental Rust reimplementation of Worms Armageddon's DDGame constructor, a deterministic replay of a longbow match produced "Checksum Mismatch" errors starting at frame 1350. Other replays (bots 3v3, shotgun) passed. This documents the investigation that ultimately traced the root cause to a single missing sprite load (`debris.spr`) in headless mode, and the debugging methodology used to find it.

## Background

OpenWA replaces WA functions with Rust reimplementations via DLL injection. The DDGame constructor (~900 lines of Rust replacing a ~19KB x86 function) initialises the entire game engine: display layers, terrain, sprites, sound, HUD, and entity systems. An A/B toggle (`OPENWA_USE_ORIG_CTOR=1`) switches between the Rust and original constructors for comparison.

Replay testing works by running WA.exe with a recorded `.WAgame` file in headless mode (no graphics window), capturing the game log, and comparing against a known-good baseline. WA's built-in checksum system sends periodic state checksums during gameplay; mismatches indicate simulation divergence.

## The Symptom

- Rust constructor: **4 checksum mismatches** at frames 1350, 2186, 2249, 2775
- Original constructor: all checksums pass
- DDGame flat memory (39KB): **byte-for-byte identical** between both constructors
- Terrain collision bitmaps: identical
- Entity tree: same 68 entities in same BFS order

The game outcome was the same (same winner, same damage), but the checksums diverged.

## Dead Ends

Several hypotheses were investigated and ruled out:

1. **Landscape bitmap differences** — dumped 1.3MB terrain collision bitmaps from both runs: byte-identical at construction AND at the divergence frame.

2. **Entity tree ordering** — full entity snapshot comparison showed ~490 diff lines. Seemed like proof of different entity ordering. But the same comparison at a *known-good* frame also showed ~490 diffs, and the bots replay (which passes) showed ~480 diffs. The snapshot tool's pointer canonicalization heuristic (`is_likely_pointer` using `can_read()`) was producing false positives. **Lesson: always validate your diff tool against a known baseline.**

3. **PaletteContext buffer** — the original constructor creates a PaletteContext on the stack and reuses it; our code creates a fresh one. Tried matching the initialization: no effect. The cached GfxDir path was taken, bypassing the PaletteContext entirely.

4. **PCLandscape constructor parameters** — verified via x86 disassembly that `param_5 = game_info + 0xDAAC` (not raw game_info). Our code was already correct.

## The Breakthrough: Sub-Object Diffing

The key insight: DDGame's flat memory matching doesn't mean *everything* matches. The constructor calls WA functions that modify objects *outside* DDGame — particularly the **display object** (DDGameWrapper+0x4D0).

Dumping the display object (16KB) before and after construction revealed:

| Offset | Before | After (orig) | After (rust) |
|--------|--------|-------------|-------------|
| +0x09C0 | 0 | **3** | 0 |
| +0x353C | 0 | **1** | 0 |

The display started identical and diverged *during* the constructor.

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
if (*wrapper).gfx_mode != 0 {
    DDDisplay::load_sprite_by_layer(disp, 3, 0x26D, land_layer, "back.spr");
    DDDisplay::load_sprite(disp, 3, 0x26E, 0, land_layer, "debris.spr");
}
```

In headless mode (`gfx_mode = 0`), both sprites were skipped. But the original WA constructor loads them unconditionally — even without a graphics window. Sprite 0x26E is `debris.spr`, used by `GenerateDebrisParticles` (0x546F70) for particle effects.

`GenerateDebrisParticles` reads terrain pixels and, when a hit is detected, updates the **game RNG** (DDGame+0x45EC) and creates particle entities. Without `debris.spr` loaded in the display, the sprite lookup returns different data, causing different RNG update patterns:

- Original: 15,470 RNG writes (from frame 808 onward)
- Rust (broken): 17,106 RNG writes
- Delta: 1,636 extra RNG ticks → cascading simulation divergence

## The Partial Fix (and a Lesson in Verifying Results)

```rust
// Load unconditionally — debris.spr affects game RNG via GenerateDebrisParticles
DDDisplay::load_sprite(disp, 3, 0x26E, 0, land_layer, c"debris.spr".as_ptr());
```

This change is correct — the original constructor loads `debris.spr` unconditionally, and our code should match. However, it did **not** resolve the desync. An initial test appeared to show 3 of 4 mismatches fixed, but this was a `tail -5` truncation error — only the last mismatch was visible, creating a false impression. All 4 mismatches persist.

The display state differences (sprite layer counters) we found were real and worth fixing for correctness, but they turned out not to be the root cause of the checksum mismatch. The investigation continues.

## Debugging Tools Built

The investigation produced several reusable tools:

- **Hardware watchpoint system** with EBP-based stack trace walking (VEH + DR0–DR3)
- **Per-frame game state hashing** via forced `SerializeGameState` calls
- **Binary sub-object dumping** for display, GfxHandler, PCLandscape comparison
- **`OPENWA_WATCH_DISPLAY=1`** env var to arm watchpoints on the display object during construction
- Terrain collision bitmap dumping at arbitrary frames

## Key Lessons

1. **Identical struct memory ≠ identical behaviour.** The constructor's side effects on *other* objects matter as much as the struct it fills. Compare sub-objects systematically.

2. **Validate your diff tools.** Our snapshot comparison showed ~490 "differences" at known-good frames — pure noise from pointer canonicalization heuristics. Always baseline.

3. **There is no "visual-only" code in a deterministic game engine.** `debris.spr` seems purely decorative — it controls what debris particles look like when terrain is destroyed. But Worms uses a **single shared RNG** for everything: gameplay physics, AI decisions, particle effects, sound variation. There is no separate "visual RNG." Every RNG consumer must execute identically, or the streams diverge and every subsequent random outcome differs. Skipping a single sprite load changed how `GenerateDebrisParticles` consumed RNG ticks, shifting the entire RNG sequence for all downstream gameplay.

4. **Hardware watchpoints are underrated.** x86 DR0–DR3 with a VEH handler gives "what wrote this byte?" answers in seconds, without an external debugger. Adding stack traces made them even more powerful.

5. **Binary search on frames, not code.** Per-frame RNG logging narrowed 542 candidate frames to the exact frame (818) where divergence began, before any code analysis.
