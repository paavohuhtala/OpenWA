# OpenWA

## Project

OpenWA (short for "Open Worms Armageddon") is an incremental Rust reimplementation of Worms Armageddon 3.8.1 (Steam). The replacement strategy is the same as was used in OpenRCT2: a custom launcher (`openwa-launcher`) injects a DLL (`openwa-dll`) into a suspended game process, which replaces game functions with Rust implementations (from `openwa-game`).

The original WA.exe is a 32-bit x86 Windows PE binary built with MSVC 2005 + MFC. All Rust code targets `i686-pc-windows-msvc`.

## Crate Architecture

- **`openwa-core`** — Cross-platform, idiomatic Rust fundamentals. No WA.exe memory references, no Ghidra addresses, no Windows APIs. Currently hosts: `fixed` (16.16 `Fixed` newtype), `log` (file-logging helper), `rng` (WA's LCG PRNG), `scheme` (.wsc parser), `sprite_lzss` (LZSS decompressor), `trig` (sin/cos tables copied byte-for-byte from WA.exe .rdata plus interpolation helpers), `weapon` (Weapon/FireType/FireMethod/SpecialFireSubtype enums). New portable modules migrate here from `openwa-game` as they're confirmed platform-independent. See `crates/openwa-core/CLAUDE.md` for the charter.
- **`openwa-game`** — WA.exe-specific code (`i686-pc-windows-msvc` only). Types, addresses, parsers, ASLR rebasing, typed WA function wrappers, **and game logic**. The source of truth for all reverse-engineered type layouts, known addresses, and Rust reimplementations of WA functions. Contains `registry` (structured address database + field registries), `rebase` (ASLR delta), `wa_call` (calling convention helpers), `wa/` (typed handle wrappers), and game logic modules (`game/weapon_fire.rs`, `game/weapon_release.rs`, `audio/sound_ops.rs`, `engine/team_ops.rs`).
- **`openwa-derive`** — Proc macro crate. Provides `#[derive(FieldRegistry)]` for struct field maps and `#[vtable(...)]` for typed vtable definitions with introspection, calling wrappers, and replacement support.
- **`openwa-dll`** — Injected DLL (`openwa.dll`): thin hook installation shims (trampolines, `usercall_trampoline!`, `install()`) that wire core's game logic into WA.exe via MinHook. Logs to `OpenWA.log`. Runs registry-driven startup checks automatically at load.
- **`openwa-test-runner`** — Headless replay test runner (`openwa-test` binary). Discovers replay tests, runs them concurrently via WA.exe's `/getlog` mode, compares output logs. See "Replay Testing" section.
- **`openwa-launcher`** — Launches WA.exe with the DLL injected via CREATE_SUSPENDED + remote thread.
- **`openwa-debugui`** — In-process egui debug window (entity census, struct inspector, cheats). Enabled via `OPENWA_DEBUG_UI=1` + `debug-ui` cargo feature.
- **`openwa-debug-cli`** — CLI tool for live memory inspection (`openwa-debug` binary). Connects to the debug server in the DLL.
- **`openwa-debug-proto`** — Shared protocol types (Request/Response enums, MessagePack framing) between CLI and server.

## Build & Test

Build: `cargo build --release` (default target is `i686-pc-windows-msvc` via `.cargo/config.toml`).

Unit tests: `cargo test`. These are standard Rust tests covering parsers and type logic.

**Replay tests are the primary verification method** — see below.

## Replay Testing

WA.exe can deterministically replay recorded games (`.WAgame` files). Each replay test runs the game with the injected DLL and checks that the output matches a baseline log (`*_expected.log`) captured from unmodified WA.exe.

Two ways to run replay tests:

### Headless (`.\run-tests.ps1`)

Pure CPU simulation, no rendering. Fast, runs in parallel (default 4 concurrent). Validates game logic — a log mismatch means the Rust code caused a desync. Spurious flakes from race conditions can occur; retry with `-j 1` to confirm.

### Headful (`.\replay-test.ps1` or `openwa-test headful`)

Runs with graphics and sound. The game window must be focused once to start. Needed to validate visual/audio hooks — code can pass headless but crash headful (or vice versa) since rendering and sound paths are only exercised headfully. Checks for crashes, panics, and `[GAMEPLAY PASS/FAIL]` markers. Timeout configurable with `--timeout SECS` (default 150s).

See `crates/openwa-test-runner/CLAUDE.md` for test isolation, crash detection, adding new tests, and env vars. Use `/desync-debug` skill after a test failure to diagnose.

### Use replay testing to validate assumptions and test theories

Implement a hypothesis, run tests, iterate. You can add temporary log statements and see their results by running replay tests.

### **IMPORTANT**: Replay tests can only test two things:

- The game simulation matches the original WA.exe, to the extent covered by the replay log and the replay's built-in checksums
- The game doesn't crash

You, as an agent, CANNOT see or hear the game, and therefore you can't verify that anything related to graphics or audio is correct. A passing headful replay test is NOT a guarantee that the rendering is correct. You need to involve the user to verify all graphics and audio related changes before committing or merging.

### **IMPORTANT:** The `*_expected.log` baselines are ground truth from unmodified WA.exe. They must NEVER be deleted or regenerated, unless explicitly requested by the user. If a test fails, the Rust code is wrong.

## Debug CLI

Use the `/debug-cli` skill for live memory inspection, struct inspection, frame-level control, and pointer chain traversal.

## Hardware Watchpoints

`crates/openwa-dll/src/debug_watchpoint.rs` — x86 debug register instrumentation (DR0-DR3) for answering "who writes this memory?" without an external debugger. Uses INT3→VEH trick, logs symbolicated stack traces via `registry::format_va()`. Max 4 watchpoints per run (hardware limit).

**API:** `arm_watchpoint(base_ptr, offset, size)` sets a write watchpoint. `prepare()` initializes the VEH handler, `teardown()` removes it. Can be armed at any point during execution — on object allocation, at a specific frame via env var, or manually from debug server commands.

**Env vars:** `OPENWA_WATCH_FRAME=N` (arm at frame N), `OPENWA_WATCH_WRAPPER=1` (base=DDGameWrapper), `OPENWA_WATCH_DISPLAY=1` (base=display object).

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

## Hooking & Desync Debugging

See `crates/openwa-dll/CLAUDE.md` for hooking patterns (passthrough, full replacement, vtable, trap), bridge function patterns, `usercall_trampoline!` macro, and hook installation details.

Use the `/desync-debug` skill for desync diagnosis (trace-desync, per-frame analysis). Only after a headless test has already detected a failure.

## Ghidra MCP

A Ghidra MCP bridge is configured in `.mcp.json`. When using Ghidra tools:

- **Prefer batch tools** (`batch_create_labels`, `batch_rename_function_components`) — single-item tools have address parsing bugs.
- WA.exe is loaded at image base 0x400000 in Ghidra.
- When you encounter unnamed functions, globals or structs, name them in Ghidra if you know their purpose. Even a guess is helpful for future reference, but add `_Maybe` suffix if uncertain.
- Remove `_Maybe` suffix when you confirm the purpose.
- When you learn more about a function or address, update both the Ghidra database (rename function / label and update signature) and the corresponding Rust code.

## Address Registry, FieldRegistry & Vtable Macros

See `crates/openwa-game/CLAUDE.md` for the full reference on `define_addresses!`, `#[derive(FieldRegistry)]`, `#[vtable(...)]`, `vtable_replace!`, and the registry query API.

## Design Conventions

- Unknown struct fields as `_unknown_XX` padding arrays
- Fixed-point: `Fixed(i32)` newtype, 16.16 format (0x10000 = 1.0)
- Naked asm uses `naked_asm!` (Rust 1.79+ syntax), not `asm!`
- **Generic CTask<V> for typed vtables**: `CTask<V: Vtable = *const c_void>` and `CGameTask<V: Vtable = *const c_void>` take a vtable pointer type parameter. Subclasses specify their typed vtable: `CTaskTeam { base: CTask<*const CTaskTeamVTable> }`. The `Vtable` marker trait is auto-implemented by the `#[vtable]` macro. `FieldRegistry` derive supports generics by substituting type params with their defaults for `offset_of`/`size_of`.
- **`Task` trait**: Provides `task()`, `ddgame()`, `as_task_ptr()`, `as_task_ptr_mut()`, and `broadcast_message()` on all CTask subclasses. Eliminates `.base.base` chains. Implemented for all task types in `task/mod.rs`.
- **`broadcast_message()` / `broadcast_message_raw()`**: Pure Rust port of CTask::HandleMessage (0x562F30). Iterates the sparse children array and calls each child's vtable[2]. Uses `read_volatile` for `children_watermark` and `children_data` — **required** because LLVM caches these reads across virtual dispatch calls even through `*mut`. Prefer `CTask::broadcast_message_raw(ptr, ...)` over `(*ptr).broadcast_message(...)` — see noalias rule above.
- **CTask base vtable**: 7 slots (not 8). ProcessFrame is slot 6. Slots 7+ are class-specific extensions. CGameTask adds no virtuals.
- **Noalias rule — ALWAYS use `_raw` methods on WA objects**: `bind_!` methods with `&mut self` give LLVM noalias guarantees that WA bridge calls violate, causing silent miscompilation. **Always use `Type::method_raw(ptr, args)` instead of `(*ptr).method(args)`**. Similarly use `CTask::ddgame_raw(ptr)` instead of `(*ptr).ddgame()`, and `CTask::broadcast_message_raw(ptr, ...)` instead of `(*ptr).broadcast_message(...)`. For `as_task_ptr`, just cast: `worm as *mut CTask`.
- **`vcall!` macro**: Still available for raw-pointer vtable dispatch without bind wrappers. Expands to `((*(*obj).vtable).method)(obj, args...)`.

## FFI Style

Add type safety incrementally where it's beneficial — this is a reverse engineering project, not a greenfield codebase. Perfect types aren't always possible, but small improvements compound.

- **Structs over raw memory**: Create `#[repr(C)]` structs for known memory layouts. Access fields by name, not pointer arithmetic. Even partially-known structs (with `_unknown_XX` padding) are better than raw offsets.
- **Constant improvements**: Adding a new struct or a missing field (by splitting an existing `_unknown` field), renaming a field from `_unknown` to a descriptive name, replacing an integer or void pointer with a better type, or labeling a function in Ghidra are ALWAYS welcome improvements. Small improvements to documentation and type safety should NEVER be considered out of scope. Improving the codebase is more important than keeping change sets small.
- **Typed pointers over integers**: Function signatures (ported functions, FFI wrappers, hook impls) MUST use pointer types for pointer arguments, never `u32`. If the pointee struct exists in Rust, use it (`*mut DDGame`, `*mut PaletteContext`). If it doesn't exist yet, strongly consider adding an incomplete `#[repr(C)]` struct with `_unknown_XX` padding — even a stub enables typed field access later. Use `*mut u8` / `*const u8` ONLY when the pointee is truly untyped (generic memory like `memcpy`, raw byte buffers). Use `*const c_char` for C string pointers. Note: on our target (`i686-pc-windows-msvc`), `u32 == usize == *const T == *mut T` are all 4 bytes, so casts between them are always safe and zero-cost.
- **Constants over magic numbers**: Name addresses (`va::FESFX_WAV_PLAYER`), sizes (`MAX_PATH`), and offsets. Magic numbers in code should be rare and commented. Use typed enums (`Weapon`, `KnownSoundId`, `FireType`, `SpecialFireSubtype`) and newtypes (`Fixed`, `SoundId`) instead of raw `u32`/`i32`.
- **Wrap inline asm in safe-to-call functions**: Isolate `asm!` / `naked_asm!` blocks in small dedicated functions (e.g., `get_team_config_name()`, `wav_player_stop_raw()`). Hook functions should read like normal Rust, calling into asm wrappers only when needed.
- **ESI/EDI are LLVM-reserved on x86**: Cannot use `in("esi")` or `in("edi")` in `core::arch::asm!`. Use `#[unsafe(naked)]` functions with `naked_asm!` when these registers are needed.
- **`heapless::CString<N>`** for stack-allocated null-terminated path buffers (auto nul terminator, `as_ptr()` returns `*const c_char`).
