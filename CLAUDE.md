# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

OpenWA is an incremental Rust reimplementation of Worms Armageddon 3.8.1 (Steam). The strategy is "parasite" — WormKit DLLs injected into the running game that progressively replace functions, starting from self-contained "leaf" functions and working inward.

**Target:** WA.exe is a 32-bit x86 Windows PE binary built with MSVC 2005 + MFC. All Rust code targets `i686-pc-windows-msvc`.

## Build & Test

```bash
# Build everything (default target is i686-pc-windows-msvc via .cargo/config.toml)
cargo build --release

# Build a specific crate
cargo build -p openwa-wormkit --release

# Run tests (openwa-types is pure Rust, works on any host)
cargo test -p openwa-core

# Single test
cargo test -p openwa-core -- scheme_parse::parse_beginner_v2
```

## How to run the game with the DLL

It used to be necessary to copy the built DLLs to the game directory and launch WA.exe. **HOWEVER**, we now have the launcher crate (`openwa-launcher`) that automatically starts the game with the correct DLL injected.

## Replay Testing

**Use replay testing to validate assumptions and test theories!** Make a hypothesis, implement it in the DLL, then run replay tests to see if it holds up against the real game. This iterative approach is much faster than trying to get everything right from static analysis alone.

### Headless test runner (primary)

The `openwa-test-runner` crate (`openwa-test` binary) runs all replay tests automatically with concurrent execution:

```bash
# Run all tests (builds everything, default 4 concurrent):
.\run-tests.ps1

# Filter by name, control parallelism:
.\run-tests.ps1 longbow         # only tests matching "longbow"
.\run-tests.ps1 -j 1            # serial mode
.\run-tests.ps1 --no-build      # skip internal DLL/launcher build
```

Each test runs WA.exe in headless `/getlog` mode (pure CPU simulation, no rendering) and compares the output log byte-for-byte against an expected baseline.

**Test isolation** (all in headless mode only):
- Per-PID temp directory: `.openwa_tmp/{pid}/` for writable files (writetest.txt, mono.tmp, land.dat, landgen.svg, etc.)
- Per-PID named event: `OpenWA_HooksReady_{pid}` for launcher↔DLL synchronization
- Per-PID semaphore: `CreateSemaphoreA("Worms Armageddon")` renamed to `Worms Armageddon_{pid}`
- Per-PID log paths via `OPENWA_LOG_PATH` and `OPENWA_ERRORLOG_PATH` env vars
- Per-PID file redirection via `file_isolation.rs` (CreateFileA hook redirects playback.thm, current.thm, land.dat, landgen.svg to `.openwa_tmp/{pid}/`)
- WA.exe crash dialog suppressed via `/silentcrash` command-line flag
- Batched MinHook enables: all hooks use `queue_enable_hook` + single `apply_queued()` call

**Crash detection**: Tests that crash show `CRASH` (not `FAIL`) with NTSTATUS name and ERRORLOG.TXT content. ERRORLOG.TXT is redirected to the per-test run directory via `OPENWA_ERRORLOG_PATH`.

**IMPORTANT:** The `*_expected.log` baselines are ground truth generated from unmodified WA.exe. They must NEVER be deleted or regenerated. If a test fails, the Rust code is wrong.

Key paths:
- Replay files + expected logs: `testdata/replays/*.WAgame` + `*_expected.log`
- Per-run output: `testdata/runs/<timestamp>/` (gitignored)
- Test runner: `crates/openwa-test-runner/`
- Convenience script: `run-tests.ps1`

### Headful replay testing (interactive)

Use the `/replay-test` skill for interactive testing with graphics and sound:

```bash
powershell -ExecutionPolicy Bypass -File replay-test.ps1
powershell -ExecutionPolicy Bypass -File replay-test.ps1 -Headless testdata/replays/longbow.WAgame
```

Headful mode enables fast-forward (50x via DDGame+0x98B0). Use for debugging specific replays or testing visual/audio hooks.

### Environment variables

- `OPENWA_HEADLESS=1` — Headless mode: hooks MessageBoxA to auto-dismiss, launcher uses SW_HIDE, file isolation hook active, semaphore renamed per-PID
- `OPENWA_REPLAY_TEST=1` — Fast-forward mode for headful testing (50x speed, 120s safety timeout)
- `OPENWA_LOG_PATH=<path>` — Override OpenWA.log location (used by test runner for per-instance isolation)
- `OPENWA_ERRORLOG_PATH=<path>` — Redirect ERRORLOG.TXT to specified path (used by test runner for crash capture)
- `OPENWA_TRACE_DESYNC=1` — Enable per-frame checksum logging (hooks GameFrameChecksumProcessor)
- `OPENWA_TRACE_BASELINE=1` — Baseline mode: skip all gameplay hooks, keep only infrastructure
- `OPENWA_TRACE_HASH_PATH=<path>` — Override frame hash log location (default: frame_hashes.log)

### Trace-desync (automated desync detection)

The `trace-desync` subcommand hooks WA's own `GameFrameChecksumProcessor` (0x5329C0) to capture per-frame checksums, runs the game twice (baseline with minimal hooks vs all hooks), and diffs the results:

```bash
.\trace-desync.ps1 testdata/replays/longbow.WAgame

# Or directly:
openwa-test trace-desync testdata/replays/longbow.WAgame [--no-build] [--wa-path PATH]
```

Baseline mode (`OPENWA_TRACE_BASELINE=1`) installs only: headless, file_isolation, frame_hook, trace_desync. All gameplay hooks (replay, weapon, sound, constructor, etc.) are skipped, giving a "nearly vanilla" WA reference run.

Output reports the first divergent frame or confirms all checksums match. Per-frame hash logs are saved in `testdata/runs/trace-<timestamp>/`.

### Adding new replay tests

1. Record a game in WA.exe (the replay `.WAgame` file is saved automatically)
2. Copy the replay to `testdata/replays/`
3. Run once with the headless test runner — it auto-generates the `*_expected.log` baseline
4. Subsequent runs compare against this baseline

## Debug CLI

Use the `/debug-cli` skill to inspect live game memory, set frame breakpoints, and capture game state snapshots. The debug server runs inside the DLL; the CLI is a separate binary that connects over TCP.

```bash
# Start game in debug mode:
powershell -ExecutionPolicy Bypass -File start-debug.ps1

# Or with headless replay + frame breakpoint:
OPENWA_BREAK_FRAME=1350 OPENWA_DEBUG_SERVER=1 \
  powershell -File replay-test.ps1 -Headless testdata/replays/longbow.WAgame
```

Key commands:
- `openwa-debug read "0x7A0884->0xA0->0x488" 0x100` — read memory via pointer chains
- `openwa-debug inspect DDGame ddgame` — typed struct inspection with named fields
- `openwa-debug inspect CTaskWorm "ddgame->task_land"` — follow field-name pointer chains
- `openwa-debug objects` — list tracked live objects (DDGame, DDGameWrapper, GameSession)
- `openwa-debug suspend` / `resume` / `step N` — frame-level control
- `openwa-debug snapshot` — canonicalized game state dump (for diffing)
- `openwa-debug break 1350` — set frame breakpoint

Address syntax: Ghidra VAs (auto-rebased), `abs:` prefix for absolute, `+offset` / `[offset]`, `->` for pointer chains. **Always quote chain addresses** (`"0x7A0884->0xA0->0x488"`).

### Symbolic addresses

Named aliases and field names can be used anywhere an address is accepted:
- `ddgame` — resolves to DDGame's runtime address (any tracked live object name works, case-insensitive)
- `ddgame+frame_counter` — DDGame base + field offset (no deref)
- `ddgame->task_land` — follow the task_land pointer (offset + deref)
- `gamesession->ddgame_wrapper->display` — multi-step field chains
- Mixed: `ddgame->0x54C` still works (hex offsets alongside names)

Field names are resolved via the server's FieldRegistry, including CTask inheritance chains.

Key env vars:
- `OPENWA_DEBUG_SERVER=1` — enable TCP debug server (port 19840)
- `OPENWA_BREAK_FRAME=N` — auto-pause at frame N
- `OPENWA_USE_ORIG_CTOR=1` — use original WA DDGame constructor (for A/B testing)
- `OPENWA_WATCH_FRAME=N` — arm hardware watchpoint on DDGame at frame N
- `OPENWA_WATCH_WRAPPER=1` — watchpoint base = DDGameWrapper instead of DDGame
- `OPENWA_WATCH_DISPLAY=1` — watchpoint base = display object (DDGameWrapper+0x4D0), armed during constructor

## Crate Architecture

- **`openwa-core`** — Types, addresses, parsers, ASLR rebasing, typed WA function wrappers, **and game logic**. The source of truth for all reverse-engineered type layouts, known addresses, and Rust reimplementations of WA functions. Contains `registry` (structured address database + field registries), `rebase` (ASLR delta), `wa_call` (calling convention helpers), `wa/` (typed handle wrappers), and game logic modules (`game/weapon_fire.rs`, `game/weapon_release.rs`, `audio/sound_ops.rs`, `engine/team_ops.rs`).
- **`openwa-derive`** — Proc macro crate. Provides `#[derive(FieldRegistry)]` for struct field maps and `#[vtable(...)]` for typed vtable definitions with introspection, calling wrappers, and replacement support.
- **`openwa-wormkit`** — Unified WormKit cdylib: thin hook installation shims (trampolines, `usercall_trampoline!`, `install()`) that wire core's game logic into WA.exe via MinHook. Logs to `OpenWA.log`. Runs registry-driven startup checks automatically at load.
- **`openwa-test-runner`** — Headless replay test runner (`openwa-test` binary). Discovers replay tests, runs them concurrently via WA.exe's `/getlog` mode, compares output logs. See "Replay Testing" section.
- **`openwa-debug-cli`** — CLI tool for live memory inspection (`openwa-debug` binary). Connects to the debug server in the DLL.
- **`openwa-debug-proto`** — Shared protocol types (Request/Response enums, MessagePack framing) between CLI and server.

## ASLR Rebasing

WA.exe has ASLR enabled. Ghidra shows addresses at image base 0x400000, but runtime base varies. Both DLLs compute a delta at startup:

```rust
let base = GetModuleHandleA(NULL) as u32;
let delta = base.wrapping_sub(0x400000);
// rb(ghidra_addr) = ghidra_addr + delta
```

All addresses in `address.rs` are Ghidra VAs. Always use `rb()` to convert to runtime addresses.

## Calling Convention Rules

These are critical — wrong conventions cause stack corruption and crashes.

**The Ghidra decompiler is UNTRUSTWORTHY for calling conventions.** It frequently misidentifies stdcall/thiscall/usercall. Always verify via disassembly: check the `RET imm16` instruction AND the caller's register setup at the call site.

- **Constructors are usually `__stdcall`** — `this` passed on stack, not ECX. **Exception**: CTaskMissile constructor (0x507D10) is `__thiscall` (ECX=this, 3 stack params, RET 0xC). Always verify by checking the call site's register setup.
- **VTable methods are `__thiscall`**: ECX = this, remaining params on stack.
- **Always check `RET imm16`** in disassembly to verify stack parameter count. The immediate value = bytes of params cleaned by callee (stdcall/thiscall). `RET 0x10` = 16 bytes = 4 params.
- **MSVC `__usercall`**: Some functions pass implicit params in registers (e.g., FrontendChangeScreen uses ESI for dialog pointer). These need `#[unsafe(naked)]` trampolines.

## Hooking Patterns

Hooks use the `minhook` crate. Two patterns:

1. **Passthrough hook** (logging only): Call original via trampoline, log result. See `replacements/scheme.rs`.
2. **Full replacement**: Reimplement the function in Rust, call WA functions via `wa_call` helpers. See `replacements/frontend.rs`.

For `__usercall` functions, use a naked trampoline to capture register params before calling the Rust impl. **ECX preservation**: The standard `reg = ecx` trampoline variants do NOT preserve ECX across the cdecl impl call. MSVC-generated callers often loop calling thiscall functions without re-setting ECX between iterations (relying on the original function preserving it). Use the `preserve_ecx` variant for thiscall hooks where callers may rely on ECX being preserved: `usercall_trampoline!(fn name; impl_fn = path; reg = ecx; stack_params = N; ret_bytes = "0xN"; preserve_ecx)`.

3. **Vtable method replacement**: Use `vtable_replace!` to patch vtable slots at runtime. Write the replacement as `unsafe extern "thiscall" fn`. For base-class call-through, either save the original via `[ORIG_STATIC]` syntax, or call `broadcast_message()` for CTask::HandleMessage. See `replacements/task/cloud.rs`.
4. **Trap hook** (`install_trap!`): For functions whose only caller is now ported Rust. Panics if called unexpectedly. Used for FireWeapon (0x51EE60) after WeaponRelease was ported.

### Bridge function patterns

When calling unported WA functions from Rust, use naked asm bridges. **Always pass the runtime target address as a cdecl parameter** (e.g., `rb(va::FUNC_ADDR)` as the last arg). Do NOT use `sym` + `jmp [ptr]` indirection through static pointers — this causes crashes due to relocation/PIC issues on x86 DLLs.

When a register param (e.g., EAX) must be set for the target function, load the call target into a **different** register (EBX, etc.) before the `call`. Do not use EAX for the target if EAX is also a parameter.

## Desync Debugging

Replay desyncs (checksum mismatches) can be caused by any code difference — constructor side effects, hooked function behaviour, missing state, wrong calling conventions, etc. Key methodology:

0. **Start with `trace-desync`**: Run `.\trace-desync.ps1 testdata/replays/<replay>.WAgame` to automatically find the exact frame where baseline and hooked runs diverge. This replaces manual per-frame RNG logging as the first diagnostic step.
1. **WA uses a single shared RNG** (DDGame+0x45EC, `AdvanceGameRNG` at 0x53F320) for both gameplay AND visual effects. There is no separate "visual RNG." Even purely decorative things like particle sprites affect the game RNG and will cause desyncs in headless mode if handled differently. A secondary effect RNG exists at DDGame+0x45F0 (`advance_effect_rng()`, simpler LCG without frame_counter) — used by WeaponRelease visual effects. Uses `team_health_ratio[0]` (unused index-0 slot).
2. **DDGame flat memory matching is NOT sufficient.** Constructors and hooks have side effects on sub-objects (display, GfxHandler, PCLandscape). Compare all objects pointed to by DDGame AND DDGameWrapper.
3. **Use hardware watchpoints** (`debug_watchpoint.rs`) with stack traces to find what writes a specific field. DR0–DR3 + VEH handler gives "who wrote this byte?" answers without an external debugger.
4. **Per-frame RNG logging** (DDGame+0x45EC) pinpoints the exact frame where simulation diverges. Binary search on frames, not code.
5. **Always validate diff methodology** against a known-good frame first. The snapshot system's pointer canonicalization produces false positives.

See `docs/re-notes/desync-investigation.md` for a detailed case study.

## Hardware Watchpoint Debugger

`crates/openwa-wormkit/src/debug_watchpoint.rs` — self-contained x86 debug register instrumentation. Sets DR0–DR3 write watchpoints on DDGame offsets via an INT3→VEH trick (no external debugger needed). Logs symbolicated stack traces via `registry::format_va()` (e.g., `CONSTRUCT_DD_GAME+0x220` instead of raw hex).

**Usage:** Call `prepare()` + `on_ddgame_alloc(ptr)` around the constructor, `teardown()` after. For the original WA constructor, use `prepare_with_malloc_hook()` which intercepts `wa_malloc(0x98D8)` to arm watchpoints from inside the constructor. Configure offsets in `WATCH_OFFSETS` (max 4 per run, hardware limit).

Currently dormant — no hooks wired up. Activate by adding calls in `game_session.rs`.

## Key Files

- `crates/openwa-core/src/address.rs` — Known WA.exe addresses (segment boundaries + re-exports from home modules)
- `crates/openwa-core/src/registry.rs` — Structured address registry, field registries (`ValueKind`), live object tracker, query API
- `crates/openwa-core/src/field_format.rs` — `FieldFormatter` trait, `format_field()`, default formatters for all `ValueKind` variants
- `crates/openwa-core/src/macros.rs` — `define_addresses!`, `vcall!`, and `vtable_replace!` macros
- `crates/openwa-core/src/mem.rs` — Pointer classification and `identify_pointer()` for debug tools
- `crates/openwa-core/src/wa_call.rs` — Helpers for calling WA functions (thiscall, stdcall wrappers)
- `crates/openwa-core/src/rebase.rs` — ASLR delta computation
- `crates/openwa-core/src/wa/` — Typed WA function wrappers (MFC, frontend, registry, DDGame)
- `crates/openwa-core/src/task/base.rs` — CTask<V>, CTaskVtable (7 slots), Task trait, SharedDataTable, broadcast_message
- `crates/openwa-core/src/task/worm.rs` — CTaskWorm, CTaskWormVTable (20 slots), WormState enum (all gaps filled)
- `crates/openwa-core/src/game/weapon.rs` — Weapon enum, WeaponEntry, FireType/FireMethod/SpecialFireSubtype enums, WeaponFireParams
- `crates/openwa-core/src/audio/sound.rs` — SoundId packed type (low 16 = slot, bit 16 = loop, bit 17 = raw volume) + KnownSoundId enum (1-126 SFX)
- `crates/openwa-core/src/audio/sound_ops.rs` — Sound queue, PlaySoundLocal/Global, worm sound (stop/play), 3D distance attenuation, streaming dispatch
- `crates/openwa-core/src/engine/team_arena.rs` — TeamArenaState, WormEntry, TeamHeader, TeamArenaRef
- `crates/openwa-core/src/engine/team_ops.rs` — Team/worm state accessors (alliance counting, health, state checks)
- `crates/openwa-core/src/engine/ddgame_constructor.rs` — DDGame constructor (create_ddgame, init_graphics_and_resources)
- `crates/openwa-core/src/game/weapon_fire.rs` — FireWeapon dispatch, ammo management, 20+ special weapon handlers, projectile creation
- `crates/openwa-core/src/game/weapon_release.rs` — WeaponRelease (0x51C3D0), SpawnEffect (0x547C30) — fully ported
- `crates/openwa-wormkit/src/replacements/sound.rs` — Hook trampolines for sound functions (thin shim over sound_ops)
- `crates/openwa-wormkit/src/replacements/frame_hook.rs` — TurnManager_ProcessFrame hook, debug_sync, watchpoint arming (always installed)
- `crates/openwa-wormkit/src/replacements/replay_test.rs` — Fast-forward + gameplay milestones (headful replay testing only)
- `crates/openwa-wormkit/src/replacements/trace_desync.rs` — Per-frame checksum logging for desync bisection
- `crates/openwa-wormkit/src/replacements/` — Function replacements (one file per subsystem)
- `crates/openwa-wormkit/src/replacements/task/` — Vtable method replacements (cloud, filter, ...)
- `docs/re-notes/` — Reverse engineering documentation (task hierarchy, memory map, frontend screens)
- `docs/plans/` — Design docs and implementation plans

## Ghidra MCP

A Ghidra MCP bridge is configured in `.mcp.json`. When using Ghidra tools:
- **Prefer batch tools** (`batch_create_labels`, `batch_rename_function_components`) — single-item tools have address parsing bugs.
- WA.exe is loaded at image base 0x400000 in Ghidra.
- When you encounter unnamed functions, globals or structs, name them in Ghidra if you know their purpose. Even a guess is helpful for future reference, but add `_Maybe` suffix if uncertain.
- Remove `_Maybe` suffix when you confirm the purpose.
- When you learn more about a function or address, update both the Ghidra database (rename function / label and update signature) and the corresponding Rust code.

## Third-Party RE Sources

- `C:\koodia\worms-re\thirdparty\wkJellyWorm\` — Extensive RE data: task hierarchy, vtables, enums, weapon structs
- `C:\koodia\worms-re\thirdparty\WormKit\` — Modding framework, game state structures

## Address Registry & Pointer Identification

The `registry` module in `openwa-core` provides a structured, queryable database of known addresses. Three systems work together:

### `define_addresses!` macro

Defines known WA.exe addresses with metadata (kind, calling convention, class name). Generates both `pub const` values and `inventory`-collected `AddrEntry` items for runtime queries. Supports class blocks and standalone entries:

```rust
crate::define_addresses! {
    class "CTaskWorm" {
        vtable CTASK_WORM_VTABLE = 0x0066_44C8;
        ctor CTASK_WORM_CONSTRUCTOR = 0x0050_BFB0;
    }
    fn/Fastcall ADVANCE_GAME_RNG = 0x0053_F320;
    global G_GAME_SESSION = 0x007A_0884;
}
```

**Distributed definitions**: Each module defines its own addresses via `define_addresses!`. `address.rs` re-exports them into `mod va` for backward compatibility (`va::CTASK_WORM_VTABLE` still works). When adding new addresses, place the `define_addresses!` block in the home module, then add a `pub use` re-export in `address.rs`.

### `#[derive(FieldRegistry)]`

Auto-generates a field map for `#[repr(C)]` structs using `offset_of!()`. Fields prefixed `_unknown`/`_pad` are skipped. Applied to all key structs (DDGame, CTask, CTaskWorm, etc.). Enables runtime offset → field name lookups.

Each field gets a `ValueKind` for typed formatting, auto-inferred from the Rust type:
- `u8/u16/u32/i8/i16/i32` → scalar variants, `bool` → `Bool`, `Fixed` → `Fixed`
- `*mut T` / `*const T` → `Pointer`, `ClassType` → `Enum`
- Arrays and unknown types → `Raw`
- Override with `#[field(kind = "CString")]` for null-terminated string fields, etc.

```rust
#[derive(FieldRegistry)]
#[repr(C)]
pub struct CTask { ... }
// Generates: CTask::field_registry() -> &'static StructFields
// Also registers in global inventory for struct_fields_for("CTask")

// String fields use #[field(kind = "...")] override:
#[field(kind = "CString")]
pub worm_name: [u8; 0x11],  // Displays as "Ainsley" instead of raw hex
```

### Field formatting (`field_format.rs`)

`format_field(&mut dyn fmt::Write, data, field, ctx)` writes human-readable values based on `ValueKind`: scalars as decimal, Fixed as float (e.g., `388.43`), pointers resolved via registry (e.g., `DDDisplay*`), CString as quoted strings. Zero allocations — writes to any `fmt::Write` target.

Custom formatters can be registered via `inventory::submit!(Box::new(MyFormatter) as Box<dyn FieldFormatter>)` from any crate. The `FieldFormatter` trait has `handles() -> &[ValueKind]` and `format_field(&mut dyn Write, ...)`.

### Query API (`registry::*`)

- `lookup_va(ghidra_va)` — find address entry by VA (exact or nearest-below)
- `vtable_class_name(ghidra_vtable)` — vtable address → class name
- `format_va(ghidra_va)` — human-readable name string
- `struct_fields_for("DDGame")` / `struct_fields_for_vtable(va)` — get field map
- `field_at_inherited("CTaskWorm", offset)` — inheritance-aware field lookup (walks CTaskWorm → CGameTask → CTask)
- `identify_pointer(value, delta)` → `PointerIdentity` — full pointer identification (static addresses, live objects, vtable-based object detection)
- `register_live_object()` / `identify_live_pointer()` — track heap objects for field-level pointer resolution
- `vtable_info_for("PaletteVtable")` — vtable slot metadata (name, index, doc)

### `#[vtable(...)]` attribute macro

Defines typed vtable structs from sparse slot definitions. The macro generates the full `#[repr(C)]` struct with `usize` gap-fillers, registry metadata, a companion `bind_!` macro, and optional address constants.

```rust
#[openwa_core::vtable(size = 38, va = 0x0066_A218, class = "DDDisplay")]
pub struct DDDisplayVtable {
    /// set layer color
    #[slot(4)]
    pub set_layer_color: fn(this: *mut DDDisplay, layer: i32, color: i32),
    /// set active layer, returns layer context ptr
    #[slot(5)]
    pub set_active_layer: fn(this: *mut DDDisplay, layer: i32) -> *mut u8,
}

// Generate calling wrappers on the class struct
bind_DDDisplayVtable!(DDDisplay, vtable);
```

Key features:
- **`#[slot(N)]`** for sparse vtables — gaps auto-filled with `usize`. Optional when all slots are declared sequentially.
- **`fn(...)` shorthand** — auto-normalized to `unsafe extern "thiscall" fn(...)`.
- **Named parameters** — `fn(this: *mut T, mode: u32)` flows through to generated wrappers as `fn set_mode(&mut self, mode: u32)`. The `this` param becomes `&mut self` (or `&self` for `*const`).
- **`bind_XxxVtable!`** — companion macro generates method wrappers on the class struct.
- **`vtable_replace!`** — type-safe vtable slot patching for `install()` functions. Accepts method names (resolved via `offset_of!`) or slot indices:

```rust
vtable_replace!(DSSoundVtable, va::DS_SOUND_VTABLE, {
    play_sound [originals::PLAY] => my_play_sound,  // save original + replace
    load_wav                     => my_load_wav,     // pure replace
})?;
```

## Design Conventions

- Unknown struct fields as `_unknown_XX` padding arrays
- Fixed-point: `Fixed(i32)` newtype, 16.16 format (0x10000 = 1.0)
- Naked asm uses `naked_asm!` (Rust 1.79+ syntax), not `asm!`
- **Generic CTask<V> for typed vtables**: `CTask<V: Vtable = *const c_void>` and `CGameTask<V: Vtable = *const c_void>` take a vtable pointer type parameter. Subclasses specify their typed vtable: `CTaskTeam { base: CTask<*const CTaskTeamVTable> }`. The `Vtable` marker trait is auto-implemented by the `#[vtable]` macro. `FieldRegistry` derive supports generics by substituting type params with their defaults for `offset_of`/`size_of`.
- **`Task` trait**: Provides `task()`, `ddgame()`, `as_task_ptr()`, `as_task_ptr_mut()`, and `broadcast_message()` on all CTask subclasses. Eliminates `.base.base` chains. Implemented for all task types in `task/mod.rs`.
- **`broadcast_message()` / `broadcast_message_raw()`**: Pure Rust port of CTask::HandleMessage (0x562F30). Iterates the sparse children array and calls each child's vtable[2]. Uses `read_volatile` for `children_watermark` and `children_data` — **required** because LLVM caches these reads across virtual dispatch calls even through `*mut`. Prefer `CTask::broadcast_message_raw(ptr, ...)` over `(*ptr).broadcast_message(...)` — see noalias rule above.
- **Typed vtable structs via `#[vtable(...)]`**: Use the attribute macro to define vtable structs with `fn(this: *mut T, ...)` shorthand and `#[slot(N)]` for sparse layouts. The macro handles `unsafe extern "thiscall"`, gap-filling, registry metadata, generates `bind_!` calling wrappers, and implements the `Vtable` marker trait. See the `#[vtable(...)]` section above.
- **CTask base vtable**: 7 slots (not 8). ProcessFrame is slot 6. Slots 7+ are class-specific extensions. CGameTask adds no virtuals.
- **`bind_XxxVtable!`**: Generated by `#[vtable(...)]`, creates both `&mut self` method wrappers AND `_raw` associated functions (raw pointers) on the class struct. Supports dotted field paths for inherited vtables: `bind_CTaskTeamVTable!(CTaskTeam, base.vtable)`.
- **Noalias rule — ALWAYS use `_raw` methods on WA objects**: `bind_!` methods with `&mut self` give LLVM noalias guarantees that WA bridge calls violate, causing silent miscompilation. **Always use `Type::method_raw(ptr, args)` instead of `(*ptr).method(args)`**. Similarly use `CTask::ddgame_raw(ptr)` instead of `(*ptr).ddgame()`, and `CTask::broadcast_message_raw(ptr, ...)` instead of `(*ptr).broadcast_message(...)`. For `as_task_ptr`, just cast: `worm as *mut CTask`.
- **`vcall!` macro**: Still available for raw-pointer vtable dispatch without bind wrappers. Expands to `((*(*obj).vtable).method)(obj, args...)`.

## FFI Style

Add type safety incrementally where it's beneficial — this is a reverse engineering project, not a greenfield codebase. Perfect types aren't always possible, but small improvements compound.

- **Wrapper structs over raw values**: Create `#[repr(C)]` structs for known memory layouts. Access fields by name, not pointer arithmetic. Even partially-known structs (with `_unknown_XX` padding) are better than raw offsets.
- **Handle newtypes for opaque pointers**: When a pointer's target layout is unknown, wrap it in a newtype (e.g., `WavPlayerHandle(u32)`, `CWndHandle(u32)`) with methods that encapsulate the unsafe calls. This keeps inline asm and raw pointer work out of hook logic.
- **Typed pointers over integers**: Prefer `*mut DDGame` over `u32` for pointer parameters. Use `*const c_char` for C string pointers, not `*const u8`.
- **Constants over magic numbers**: Name addresses (`va::FESFX_WAV_PLAYER`), sizes (`MAX_PATH`), and offsets. Magic numbers in code should be rare and commented. Use typed enums (`Weapon`, `KnownSoundId`, `FireType`, `SpecialFireSubtype`) and `Fixed` for volume/pitch instead of raw `u32`/`i32`.
- **SoundId pattern**: `SoundId(u32)` is a transparent newtype for any sound ID (WA uses IDs > 126 for speech). `KnownSoundId` is the typed enum for known SFX (1-126). Sound functions accept `impl Into<SoundId>` so both work. Never reject unknown IDs in hooks — speech/voice lines use high IDs.
- **Wrap inline asm in safe-to-call functions**: Isolate `asm!` / `naked_asm!` blocks in small dedicated functions (e.g., `get_team_config_name()`, `wav_player_stop_raw()`). Hook functions should read like normal Rust, calling into asm wrappers only when needed.
- **ESI/EDI are LLVM-reserved on x86**: Cannot use `in("esi")` or `in("edi")` in `core::arch::asm!`. Use `#[unsafe(naked)]` functions with `naked_asm!` when these registers are needed.
- **`heapless::CString<N>`** for stack-allocated null-terminated path buffers (auto nul terminator, `as_ptr()` returns `*const c_char`).
