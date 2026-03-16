# DDGame Constructor Port — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace DDGame__Constructor (0x56E220) with a Rust `DDGame::new()` function that creates the 0x98B8-byte game state object field by field, bridging WA sub-calls initially.

**Architecture:** The constructor allocates DDGame via `wa_malloc(0x98B8)`, zero-fills it, then runs ~35 initialization steps covering param storage, sub-object creation (GfxHandler, PCLandscape, SpriteRegions, CoordList, DisplayGfx objects), resource loading (sprites, gradients, WAVs), and configuration. We'll implement `DDGame::new()` in openwa-core, call it from `construct_ddgame_wrapper` in game_session.rs, and bridge complex sub-calls via typed function pointers.

**Tech Stack:** Rust (i686-pc-windows-msvc), `wa_alloc` for WA heap, `core::ptr` for field writes, Ghidra MCP for decompilation reference.

---

## Chunk 1: Foundation — DDGame::new() with Parameter Storage

### Task 1: Add Missing Sub-Function Addresses

**Files:**
- Modify: `crates/openwa-core/src/address.rs`

- [ ] **Step 1: Add addresses for all sub-functions called by the constructor**

Add to `address.rs` in the appropriate section:

```rust
// --- DDGame Constructor sub-calls ---

/// DDGame__InitFields (0x526120): usercall(EDI=ddgame), plain RET.
/// Zeroes stride-0x194 table, coordinate entries, calls FUN_00526080.
pub const DDGAME_INIT_FIELDS: u32 = 0x0052_6120;  // already exists

/// FUN_00526080: usercall(ESI=ddgame), plain RET.
/// Initializes RenderQueue index arrays at +0xC4..+0x4A2.
pub const DDGAME_INIT_RENDER_INDICES: u32 = 0x0052_6080;

/// FUN_00525BE0: stdcall(ddgame_wrapper), plain RET.
/// Sets DDGame+0x7E2E/0x7E2F/0x7E3F flags based on game version/mode.
pub const DDGAME_INIT_VERSION_FLAGS: u32 = 0x0052_5BE0;

/// FUN_005411A0: unknown convention, called at start.
/// Initializes some CRT / MFC state.
pub const FUN_5411A0: u32 = 0x0054_11A0;

/// GfxHandler constructor pattern: alloc 0x19C, memset 0, set vtable.
/// vtable at GfxHandler__vtable global.
pub const GFX_HANDLER_VTABLE: u32 = 0x0066_4308; // verify in Ghidra

/// GfxHandler__LoadDir (called after fopen).
pub const GFX_HANDLER_LOAD_DIR: u32 = 0x0057_0B50; // verify

/// FUN_00570A90: called before display palette setup (non-headless).
pub const FUN_570A90: u32 = 0x0057_0A90;

/// FUN_00570E20: called after GfxHandler init.
pub const FUN_570E20: u32 = 0x0057_0E20;

/// FUN_00570F30: called before DSSound_LoadEffectWAVs.
pub const FUN_570F30: u32 = 0x0057_0F30;

/// PCLandscape__Constructor (0xB44 bytes).
pub const PC_LANDSCAPE_CONSTRUCTOR: u32 = 0x0050_56F0; // verify

/// SpriteRegion__Constructor (0x9C bytes).
pub const SPRITE_REGION_CONSTRUCTOR: u32 = 0x0057_DB20;

/// FUN_005717A0: weapon sprite loading.
pub const LOAD_WEAPON_SPRITES: u32 = 0x0057_17A0;

/// HUD_LoadWeaponSprites_Maybe.
pub const HUD_LOAD_WEAPON_SPRITES: u32 = 0x0052_4070; // verify address

/// FUN_004F6300: creates some graphics object, returns ptr or null.
pub const FUN_4F6300: u32 = 0x004F_6300;

/// FUN_004F6370: initializes TaskStateMachine-like object.
pub const FUN_4F6370: u32 = 0x004F_6370;

/// FUN_0056A830: called for non-headless display finalization.
pub const FUN_56A830: u32 = 0x0056_A830;

/// DSSound_LoadEffectWAVs (loads all sound effect WAV files).
pub const DSSOUND_LOAD_EFFECT_WAVS: u32 = 0x0057_1530; // verify

/// DSSound_LoadAllSpeechBanks.
pub const DSSOUND_LOAD_ALL_SPEECH_BANKS: u32 = 0x0057_1A70;

/// g_GameInfo global (0x7749A0).
pub const G_GAME_INFO: u32 = 0x0077_49A0;
```

- [ ] **Step 2: Verify addresses in Ghidra and label unnamed functions**

Use Ghidra MCP `batch_create_labels` / `batch_rename_function_components` for all newly identified sub-functions. Cross-reference with the decompile to confirm each address.

- [ ] **Step 3: Commit**

```bash
git add crates/openwa-core/src/address.rs
git commit -m "feat: add DDGame constructor sub-function addresses"
```

### Task 2: Add DDGameWrapper Fields Used by Constructor

**Files:**
- Modify: `crates/openwa-core/src/engine/ddgame_wrapper.rs`

The constructor accesses DDGameWrapper at offsets +0x488 (ddgame ptr), +0x48C, +0x490, +0x4C0, +0x4C4, +0x4C8, +0x4CC, +0x4D0, +0x4D8, +0x4DC, +0x4E0. Many of these are sub-object pointers set during the DDGame construction.

- [ ] **Step 1: Read current DDGameWrapper struct**

Read `crates/openwa-core/src/engine/ddgame_wrapper.rs` and identify which offsets are already mapped.

- [ ] **Step 2: Add missing DDGameWrapper fields**

Add fields for offsets used by the constructor that aren't yet mapped:
- +0x48C: Network-related object pointer (0x2C bytes, conditional)
- +0x490: Network flag byte
- +0x4C0: Primary GfxHandler pointer (0x19C bytes)
- +0x4C4: Secondary GfxHandler pointer (0x19C bytes, conditional)
- +0x4C8: GfxHandler index/flag (uint)
- +0x4CC: PCLandscape pointer (0xB44 bytes)
- +0x4D0: Display pointer (copied from DDGame display)
- +0x4D8: Unknown (set to 0)
- +0x4DC: Calculated height value
- +0x4E0: Set to -100 (0xFFFFFF9C)

- [ ] **Step 3: Verify with runtime dump**

Use the F7/validator debug tools to confirm field values at these offsets match expectations.

- [ ] **Step 4: Commit**

```bash
git add crates/openwa-core/src/engine/ddgame_wrapper.rs
git commit -m "feat: add DDGameWrapper fields used by DDGame constructor"
```

### Task 3: Implement DDGame::new() — Phase 1 (Trivial Inits + Param Storage)

**Files:**
- Modify: `crates/openwa-core/src/engine/ddgame.rs`

This is the core of the port. Phase 1 covers: allocation, zero-fill, parameter storage, and simple field writes. Complex sub-calls are bridged.

- [ ] **Step 1: Create DDGame::new() skeleton**

```rust
/// Construct a new DDGame, matching DDGame__Constructor (0x56E220).
///
/// # Safety
/// All pointer params must be valid WA objects. `wrapper` must be
/// a valid DDGameWrapper with fields initialized up to this point.
pub unsafe fn new(
    wrapper: *mut DDGameWrapper,
    keyboard: *mut DDKeyboard,
    display: *mut DDDisplay,
    sound: *mut DSSound,
    palette: *mut Palette,
    music: *mut Music,
    param7: *mut u8,      // timer? 0x1F4 observed
    net_game: *mut u8,    // from GameSession
    game_info: *mut GameInfo,
    network_ecx: u32,     // implicit ECX from caller
) -> *mut DDGame {
    // 1. Allocate and zero-fill
    let ddgame = wa_malloc(0x98B8) as *mut DDGame;
    if ddgame.is_null() {
        return core::ptr::null_mut();
    }
    core::ptr::write_bytes(ddgame as *mut u8, 0, 0x98B8);

    // 2. Call InitFields (usercall EDI=ddgame)
    call_init_fields(ddgame);

    // 3. Store at DDGameWrapper+0x488
    (*wrapper).ddgame = ddgame;

    // 4. Store constructor parameters
    (*ddgame).display = display;
    (*ddgame).sound = sound;
    (*ddgame).keyboard = keyboard;
    (*ddgame).palette = palette;
    (*ddgame).music = music;
    (*ddgame)._param_018 = param7;
    (*ddgame)._caller = network_ecx as *mut u8;
    (*ddgame).game_info = game_info;
    (*ddgame)._param_028 = net_game;

    // 5. Set global
    *(rb(va::G_GAME_INFO) as *mut *mut GameInfo) = game_info;

    // 6. Sound available + always-1 flags
    (*ddgame).sound_available =
        if (*(game_info as *const u8).add(0xF914) as *const i32).read() == 0 { 1 } else { 0 };
    (*ddgame)._field_7efc = 1;

    // ... (Phase 2 sub-calls added incrementally)

    ddgame
}
```

- [ ] **Step 2: Implement bridge for DDGame__InitFields**

InitFields is `usercall(EDI=ddgame)`, plain RET. Create a naked bridge:

```rust
/// Bridge to DDGame__InitFields (0x526120).
/// Convention: usercall(EDI=ddgame), plain RET.
#[unsafe(naked)]
unsafe extern "C" fn call_init_fields(_ddgame: *mut DDGame) {
    core::arch::naked_asm!(
        "pushl %edi",
        "movl 8(%esp), %edi",     // EDI = ddgame
        "calll *({addr})",
        "popl %edi",
        "retl",
        addr = sym INIT_FIELDS_ADDR,
        options(att_syntax),
    );
}
static mut INIT_FIELDS_ADDR: u32 = 0;
```

- [ ] **Step 3: Implement bridge for FUN_00525BE0 (version flags)**

This is `stdcall(ddgame_wrapper)` — simple transmute call:

```rust
/// Bridge to FUN_00525BE0 — sets DDGame+0x7E2E/0x7E2F/0x7E3F flags.
unsafe fn call_init_version_flags(wrapper: *mut DDGameWrapper) {
    let f: unsafe extern "stdcall" fn(*mut DDGameWrapper) =
        core::mem::transmute(rb(va::DDGAME_INIT_VERSION_FLAGS) as usize);
    f(wrapper);
}
```

- [ ] **Step 4: Commit**

```bash
git add crates/openwa-core/src/engine/ddgame.rs
git commit -m "feat: DDGame::new() phase 1 — allocation, params, trivial inits"
```

### Task 4: Wire DDGame::new() into game_session.rs

**Files:**
- Modify: `crates/openwa-wormkit/src/replacements/game_session.rs`

- [ ] **Step 1: Replace ddgame_constructor_call with DDGame::new()**

In `construct_ddgame_wrapper`, replace:
```rust
DDGAME_CTOR_ECX = input_ctrl as u32;
ddgame_constructor_call(
    this, display, sound, keyboard, palette, streaming_audio,
    timer_obj, net_game, game_info,
);
```

With:
```rust
DDGame::new(
    this, keyboard, display, sound, palette, streaming_audio as *mut Music,
    timer_obj, net_game, game_info,
    input_ctrl as u32,
);
```

- [ ] **Step 2: Remove the naked ddgame_constructor_call bridge and DDGAME_CTOR_ECX/DDGAME_CTOR_ADDR statics**

These are no longer needed. Clean up dead code.

- [ ] **Step 3: Trap DDGame__Constructor**

Add to `game_session::install()`:
```rust
hook::install_trap!("DDGame__Constructor", va::CONSTRUCT_DD_GAME);
```

- [ ] **Step 4: Build and test**

Run: `cargo build --target i686-pc-windows-msvc -p openwa-wormkit --release`

This will fail at runtime because DDGame::new() doesn't yet call all the sub-constructors. That's expected — the next tasks add them incrementally.

- [ ] **Step 5: Commit**

```bash
git add crates/openwa-wormkit/src/replacements/game_session.rs
git commit -m "refactor: wire DDGame::new() into game_session, trap original"
```

---

## Chunk 2: Sub-Object Creation Bridges

### Task 5: Network Object Init (DDGameWrapper+0x48C)

**Files:**
- Modify: `crates/openwa-core/src/engine/ddgame.rs`

- [ ] **Step 1: Add conditional network object creation**

Condition: `game_info+0xD778 == -2`. Alloc 0x2C bytes, zero-fill, store DDGame ptr at [0], copy 2 config bytes from game_info. If `network_ecx != 0`, store at `network_ecx+0x18`.

```rust
// Conditional network object (game_info+0xD778 == -2)
let d778 = *((game_info as *const u8).add(0xD778) as *const i32);
if d778 == -2 {
    let net_obj = wa_malloc(0x2C) as *mut u8;
    core::ptr::write_bytes(net_obj, 0, 0x2C);
    *(net_obj as *mut *mut DDGame) = ddgame;
    // piVar3[1..3] = 0, piVar3[6] = 0 (already zero from memset)
    *net_obj.add(0x28) = *((*(ddgame as *const u8).add(0x24) as *const *const u8)
        .read().add(0xD944));
    *net_obj.add(0x29) = *((*(ddgame as *const u8).add(0x24) as *const *const u8)
        .read().add(0xD946));
    // Store at DDGameWrapper+0x48C
    *(wrapper as *mut u8).add(0x48C).cast::<*mut u8>().write(net_obj);
    if network_ecx != 0 {
        *((network_ecx as *mut u8).add(0x18) as *mut *mut u8) = net_obj;
    }
}
```

- [ ] **Step 2: Test with replay test**

Run headful replay test — game should not crash during init.

- [ ] **Step 3: Commit**

### Task 6: GfxHandler Init + Gfx.dir Loading

**Files:**
- Modify: `crates/openwa-core/src/engine/ddgame.rs`

This is a complex section that opens Gfx.dir files, tries multiple paths, and conditionally creates a second GfxHandler. For now, bridge entirely.

- [ ] **Step 1: Create bridge for GfxHandler creation and Gfx.dir loading**

Extract the entire GfxHandler init section (lines 151-250 of decompile) into a single bridged call. This section operates on DDGameWrapper fields (+0x4C0, +0x4C4, +0x4C8), not DDGame directly, so we can bridge it as one unit.

Option A: Call the original constructor section via a calculated offset.
Option B: Reimplement the file-open logic in Rust (fopen/GfxHandler__LoadDir are simple).

Start with Option A (bridge) and convert later.

- [ ] **Step 2: Test**
- [ ] **Step 3: Commit**

### Task 7: Audio Init (Sound Effects + Speech + ActiveSoundTable)

**Files:**
- Modify: `crates/openwa-core/src/engine/ddgame.rs`

- [ ] **Step 1: Add audio initialization**

Condition: `game_info+0xF914 == 0` (not headless) AND `ddgame.sound != null`.

```rust
if *((game_info as *const u8).add(0xF914) as *const i32) == 0 {
    call_fun_570f30();  // bridge
    if !(*ddgame).sound.is_null() {
        call_dssound_load_effect_wavs();   // bridge
        call_dssound_load_all_speech_banks();  // bridge
        // Allocate ActiveSoundTable (0x608 bytes)
        let ast = wa_malloc(0x608) as *mut ActiveSoundTable;
        (*ast).ddgame = ddgame;
        (*ast).counter = 0;
        core::ptr::write_bytes(ast as *mut u8, 0, 0x600);  // zero entries only
        (*ddgame).active_sounds = ast;
    }
}
```

Note: DSSound_LoadEffectWAVs and DSSound_LoadAllSpeechBanks are already known addresses. The ActiveSoundTable allocation can be done in pure Rust since we have the struct.

- [ ] **Step 2: Test with headful replay**
- [ ] **Step 3: Commit**

### Task 8: PCLandscape + TaskStateMachine + SpriteRegions

**Files:**
- Modify: `crates/openwa-core/src/engine/ddgame.rs`

- [ ] **Step 1: Bridge PCLandscape__Constructor**

Alloc 0xB44, zero-fill, call PCLandscape constructor (complex, many params). Store at DDGameWrapper+0x4CC and DDGame+0x020.

- [ ] **Step 2: Bridge TaskStateMachine creation**

Alloc 0x2C, zero-fill, call FUN_004F6370 for init, set vtable. Store at DDGame+0x380.

- [ ] **Step 3: Bridge 8× SpriteRegion creation**

Each: alloc 0x9C, zero-fill, call SpriteRegion__Constructor with (param1, param2). Store at DDGame+0x46C..0x488 (note: these are offsets 0x46C, 0x470, 0x474, 0x478, 0x47C, 0x480, 0x484, 0x488 — the 8 sprite_regions).

SpriteRegion params (from decompile):
| DDGame offset | param1 | param2 |
|---------------|--------|--------|
| +0x474 | 0x24 | 0x2E |
| +0x46C | 0x07 | 0x2D |
| +0x470 | 0x0A | 0x0D |
| +0x478 | 0x20 | 0x00 |
| +0x47C | 0x00 | 0x00 |
| +0x484 | 0x09 | 0x173 |
| +0x488 | 0x07 | 0x1E5 |
| +0x480 | 0x07 | 0x2D |

- [ ] **Step 4: Set landscape-derived value at +0x468**

```rust
// (*ddgame).landscape_val = landscape.vtable[0xB].call()
let landscape = (*ddgame).landscape;
let vtable = *(landscape as *const *const u32);
let get_val: unsafe extern "thiscall" fn(*mut u8) -> u32 =
    core::mem::transmute(*vtable.add(0xB));
(*ddgame)._landscape_val = get_val(landscape as *mut u8) as *mut u8;
```

- [ ] **Step 5: Test**
- [ ] **Step 6: Commit**

### Task 9: Arrow Sprites + Collision Regions (32 iterations)

**Files:**
- Modify: `crates/openwa-core/src/engine/ddgame.rs`

- [ ] **Step 1: Bridge arrow sprite loading loop**

The loop loads 32 `arrow##.img` files, creates DisplayGfx objects, and creates SpriteRegion collision regions. This is complex graphics code — bridge the entire loop initially.

Create a helper that encapsulates the loop body using GfxHandler vtable calls.

- [ ] **Step 2: Test**
- [ ] **Step 3: Commit**

### Task 10: CoordList, DisplayGfx, Resource Loading

**Files:**
- Modify: `crates/openwa-core/src/engine/ddgame.rs`

- [ ] **Step 1: Create CoordList in Rust**

Pure Rust — this is simple allocation:
```rust
// CoordList: 3 u32s (count, capacity, data_ptr) + 0x12C0 buffer
let coord_list = wa_malloc(12) as *mut u32;
*coord_list = 0;            // count
*coord_list.add(1) = 600;   // capacity
let coord_data = wa_malloc(0x12C0) as *mut u8;
core::ptr::write_bytes(coord_data, 0, 0x12C0);
*coord_list.add(2) = coord_data as u32;
(*ddgame).coord_list = coord_list as *mut u8;
```

- [ ] **Step 2: Bridge coordinate list population from landscape**

The loop reads from landscape data and inserts unique entries.

- [ ] **Step 3: Bridge DisplayGfx at +0x138**

Alloc 0x2C, init via FUN_004F6370, set vtable to DisplayGfx__vtable.

- [ ] **Step 4: Bridge weapon sprite loading (FUN_005717A0 ×2)**
- [ ] **Step 5: Bridge gradient/layer/fill image loading**
- [ ] **Step 6: Bridge HUD weapon sprite loading**
- [ ] **Step 7: Set fill pixel at +0x7338**
- [ ] **Step 8: Bridge display finalization calls**
- [ ] **Step 9: Test with full headful replay**
- [ ] **Step 10: Commit**

---

## Chunk 3: Integration and Verification

### Task 11: DDGameWrapper Field Updates

**Files:**
- Modify: `crates/openwa-core/src/engine/ddgame.rs`

- [ ] **Step 1: Add DDGameWrapper field writes that happen during the constructor**

The original constructor writes to many DDGameWrapper fields:
- +0x4D8 = 0
- +0x4DC = calculated from param8+0x44C: `byte_val * 0x38 + 0x7E + 0x2AD`
- +0x4E0 = -100 (0xFFFFFF9C)
- +0x6EE8 = 0 (speech name count)

- [ ] **Step 2: Commit**

### Task 12: Full Integration Test

- [ ] **Step 1: Run headful replay test**

```bash
powershell -ExecutionPolicy Bypass -File replay-test.ps1
```

Expected: Game plays through replay without crashes. Verify audio, visuals, and turn transitions.

- [ ] **Step 2: Run headless replay test**

```bash
powershell -ExecutionPolicy Bypass -File replay-test.ps1 -Headless
```

Expected: Byte-identical log output compared to baseline.

- [ ] **Step 3: Manual gameplay test**

Start a local match, play 2-3 turns. Verify:
- Worm sprites render correctly
- Weapons fire and explode
- Sound effects play
- Turn order widget shows
- Camera follows active worm

- [ ] **Step 4: Update Ghidra labels for all newly identified functions**

Use `batch_create_labels` for any remaining unnamed functions.

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "feat: replace DDGame__Constructor with Rust DDGame::new()"
```

---

## Notes

### Bridging Strategy

For sub-calls that use unusual calling conventions (usercall with EDI/ESI), use `#[unsafe(naked)]` bridge functions matching the pattern in `game_session.rs`. For plain stdcall/thiscall sub-calls, use `core::mem::transmute` to typed function pointers.

### Order Matters

The init steps MUST execute in the exact order from the decompile. Some sub-calls depend on fields set by earlier steps (e.g., PCLandscape reads game_info from DDGame+0x24, GfxHandler is used by arrow sprite loading).

### Testing Cadence

After each Task, run a headful replay test. If anything crashes, check ERRORLOG.TXT and compare against the decompile to find which field or sub-call was missed.

### Future: Bridge Elimination (Phase 3)

Once all bridges are working, each can be independently converted to pure Rust:
1. InitFields → direct field writes (already have the offset list)
2. FUN_00526080 → Rust loop for render index arrays
3. ActiveSoundTable → already done in Task 7
4. CoordList → already done in Task 10
5. SpriteRegion / PCLandscape / GfxHandler → larger projects, future plans
