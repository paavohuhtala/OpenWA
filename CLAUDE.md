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
cargo build -p openwa-validator --release

# Run tests (openwa-types is pure Rust, works on any host)
cargo test -p openwa-core

# Run harness tests (must be 32-bit, needs WA.exe on disk)
cargo test --target i686-pc-windows-msvc -p openwa-harness

# Single test
cargo test -p openwa-core -- scheme_parse::parse_beginner_v2
```

## How to run the game with the DLL

It used to be necessary to copy the built DLLs to the game directory and launch WA.exe. **HOWEVER**, we now have the launcher crate (`openwa-launcher`) that automatically starts the game with the correct DLL injected.

## Replay Testing

Use the `/replay-test` skill to automatically build, deploy, launch WA.exe with a replay file, capture validation logs, and present results. This is the fastest way to validate struct layouts, hooks, and game state against live WA.exe.

**Use replay testing to validate assumptions and test theories!** You don't have to figure out everything from disassembly and static analysis. Make a hypothesis, implement it in the DLL, then use replay testing to see if it holds up against the real game. This iterative approach is much faster than trying to get everything right on the first try.

```bash
# Manual invocation (skill runs this automatically):
powershell -ExecutionPolicy Bypass -File replay-test.ps1
```

How it works:
1. `replay-test.ps1` builds the unified DLL, deploys to game dir, sets `OPENWA_VALIDATE=1` and `OPENWA_REPLAY_TEST=1`, launches `WA.exe` minimized with a replay file
2. The DLL restores the window after 2s via FindWindowA + ShowWindow(SW_RESTORE), then hooks TurnManager_ProcessFrame and sets DDGame+0x98B0=1 each frame to enable 50x fast-forward (same mechanism as spacebar during replay). Validation runs at 5s. The replay typically finishes in ~15-30s
3. Script copies logs to `testdata/logs/` and prints a PASS/FAIL summary

Key paths:
- Replay files: `testdata/replays/*.WAgame`
- Captured logs: `testdata/logs/` (gitignored, `validation_latest.log` / `openwa_latest.log` / `errorlog_latest.txt`)
- Crash log: WA writes `ERRORLOG.TXT` to game dir on crash -- script clears it before each run and copies to `testdata/logs/` if present
- Script: `replay-test.ps1`
- Skill: `.claude/skills/replay-test/SKILL.md`

Environment variables:
- `OPENWA_VALIDATE=1` — Enable validation module (struct checks, vtable validation, memory dumps)
- `OPENWA_REPLAY_TEST=1` — Fast-forward mode: hooks TurnManager_ProcessFrame and sets DDGame+0x98B0=1 each frame (50x speed). Restores window at 2s, runs validation at 5s, 120s safety timeout. Without this, validation runs interactively with hotkeys (F9=team blocks, F10=landscape)

## Crate Architecture

- **`openwa-core`** — Types, addresses, parsers, ASLR rebasing, and typed WA function wrappers. The source of truth for all reverse-engineered type layouts and known addresses (`address.rs`). Contains `rebase` (ASLR delta), `wa_call` (calling convention helpers), and `wa/` (typed handle wrappers like `DDGameWrapperHandle`, `CWndHandle`).
- **`openwa-wormkit`** — Unified WormKit cdylib that replaces WA functions with Rust and optionally validates types against live memory. Logs to `OpenWA.log` (hooks) and `OpenWA_validation.log` (validation). Uses MinHook for inline hooking. Validation is enabled via `OPENWA_VALIDATE=1` env var.
- **`openwa-harness`** — Offline test harness that loads WA.exe into process memory via `LoadLibraryExA(DONT_RESOLVE_DLL_REFERENCES)` for testing without running the game.

## ASLR Rebasing

WA.exe has ASLR enabled. Ghidra shows addresses at image base 0x400000, but runtime base varies. Both DLLs compute a delta at startup:

```rust
let base = GetModuleHandleA(NULL) as u32;
let delta = base.wrapping_sub(0x400000);
// rb(ghidra_addr) = ghidra_addr + delta
```

All addresses in `address.rs` are Ghidra VAs. Always use `rb()` to convert to runtime addresses.

## Calling Convention Rules

These are critical — wrong conventions cause stack corruption and crashes:

- **Constructors are `__stdcall`**, NOT `__thiscall`. `this` is passed on stack, not ECX.
- **VTable methods are `__thiscall`**: ECX = this, remaining params on stack.
- **Always check `RET imm16`** in disassembly to verify stack parameter count. The immediate value = bytes of params cleaned by callee (stdcall/thiscall). `RET 0x10` = 16 bytes = 4 params.
- **MSVC `__usercall`**: Some functions pass implicit params in registers (e.g., FrontendChangeScreen uses ESI for dialog pointer). These need `#[unsafe(naked)]` trampolines.

## Hooking Patterns

Hooks use the `minhook` crate. Two patterns:

1. **Passthrough hook** (logging only): Call original via trampoline, log result. See `replacements/scheme.rs`.
2. **Full replacement**: Reimplement the function in Rust, call WA functions via `wa_call` helpers. See `replacements/frontend.rs`.

For `__usercall` functions, use a naked trampoline to capture register params before calling the Rust impl.

## Hardware Watchpoint Debugger

`crates/openwa-wormkit/src/debug_watchpoint.rs` — self-contained x86 debug register instrumentation. Sets DR0–DR3 write watchpoints on DDGame offsets via an INT3→VEH trick (no external debugger needed). Logs the exact Ghidra VA of every write.

**Usage:** Call `prepare()` + `on_ddgame_alloc(ptr)` around the constructor, `teardown()` after. For the original WA constructor, use `prepare_with_malloc_hook()` which intercepts `wa_malloc(0x98D8)` to arm watchpoints from inside the constructor. Configure offsets in `WATCH_OFFSETS` (max 4 per run, hardware limit).

Currently dormant — no hooks wired up. Activate by adding calls in `game_session.rs`.

## Key Files

- `crates/openwa-core/src/address.rs` — All known WA.exe addresses with comments
- `crates/openwa-core/src/wa_call.rs` — Helpers for calling WA functions (thiscall, stdcall wrappers)
- `crates/openwa-core/src/rebase.rs` — ASLR delta computation
- `crates/openwa-core/src/wa/` — Typed WA function wrappers (MFC, frontend, registry, DDGame)
- `crates/openwa-wormkit/src/replacements/` — Function replacements (one file per subsystem)
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

## Design Conventions

- Unknown struct fields as `_unknown_XX` padding arrays
- Fixed-point: `Fixed(i32)` newtype, 16.16 format (0x10000 = 1.0)
- Naked asm uses `naked_asm!` (Rust 1.79+ syntax), not `asm!`
- **Typed vtable structs**: Define `#[repr(C)]` vtable structs with typed function pointers for known slots and `usize` for unknown slots (see `PaletteVtable`, `SoundEmitterVTable`). Set the class struct's vtable field to `*const FooVtable` instead of `*mut u8`.
- **`vcall!` macro**: Use `vcall!(obj, method, args...)` for one-liner vtable dispatch. Expands to `((*(*obj).vtable).method)(obj, args...)`.
- **Virtual method wrappers**: Add `impl` methods on the class struct that wrap `vcall!`. Callers write `(*obj).method(args)` — idiomatic, type-safe, and hides the vtable indirection.

## FFI Style

Add type safety incrementally where it's beneficial — this is a reverse engineering project, not a greenfield codebase. Perfect types aren't always possible, but small improvements compound.

- **Wrapper structs over raw values**: Create `#[repr(C)]` structs for known memory layouts. Access fields by name, not pointer arithmetic. Even partially-known structs (with `_unknown_XX` padding) are better than raw offsets.
- **Handle newtypes for opaque pointers**: When a pointer's target layout is unknown, wrap it in a newtype (e.g., `WavPlayerHandle(u32)`, `CWndHandle(u32)`) with methods that encapsulate the unsafe calls. This keeps inline asm and raw pointer work out of hook logic.
- **Typed pointers over integers**: Prefer `*mut DDGame` over `u32` for pointer parameters. Use `*const c_char` for C string pointers, not `*const u8`.
- **Constants over magic numbers**: Name addresses (`va::FESFX_WAV_PLAYER`), sizes (`MAX_PATH`), and offsets. Magic numbers in code should be rare and commented.
- **Wrap inline asm in safe-to-call functions**: Isolate `asm!` / `naked_asm!` blocks in small dedicated functions (e.g., `get_team_config_name()`, `wav_player_stop_raw()`). Hook functions should read like normal Rust, calling into asm wrappers only when needed.
- **ESI/EDI are LLVM-reserved on x86**: Cannot use `in("esi")` or `in("edi")` in `core::arch::asm!`. Use `#[unsafe(naked)]` functions with `naked_asm!` when these registers are needed.
- **`heapless::CString<N>`** for stack-allocated null-terminated path buffers (auto nul terminator, `as_ptr()` returns `*const c_char`).
