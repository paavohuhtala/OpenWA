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
cargo test -p openwa-types

# Run harness tests (must be 32-bit, needs WA.exe on disk)
cargo test --target i686-pc-windows-msvc -p openwa-harness

# Single test
cargo test -p openwa-types -- scheme_parse::parse_beginner_v2
```

## Deploy to Game

Game directory: `I:\games\SteamLibrary\steamapps\common\Worms Armageddon`

Copy release DLLs from `target/i686-pc-windows-msvc/release/` to the game directory:
- `openwa_wormkit.dll` → rename to `wkOpenWA.dll`
- `openwa_validator.dll` → rename to `wkOpenWAValidator.dll`

Quick deploy:
```bash
cp target/i686-pc-windows-msvc/release/openwa_wormkit.dll "I:/games/SteamLibrary/steamapps/common/Worms Armageddon/wkOpenWA.dll"
cp target/i686-pc-windows-msvc/release/openwa_validator.dll "I:/games/SteamLibrary/steamapps/common/Worms Armageddon/wkOpenWAValidator.dll"
```

WormKit auto-loads any `wk*.dll` from the game directory. Logs appear in the game directory as `OpenWA.log` and `OpenWA_validation.log`.

## Replay Testing

Use the `/replay-test` skill to automatically build, deploy, launch WA.exe with a replay file, capture validation logs, and present results. This is the fastest way to validate struct layouts, hooks, and game state against live WA.exe.

```bash
# Manual invocation (skill runs this automatically):
powershell -ExecutionPolicy Bypass -File replay-test.ps1
```

How it works:
1. `replay-test.ps1` builds both DLLs, deploys to game dir, sets `OPENWA_REPLAY_TEST=1`, launches `WA.exe` minimized with a replay file
2. The validator DLL detects the env var and enters **auto-capture mode**: waits 5s, runs all validation + dumps, then calls `ExitProcess(0)`
3. Script copies logs to `testdata/logs/` and prints a PASS/FAIL summary

Key paths:
- Replay files: `testdata/replays/*.WAgame`
- Captured logs: `testdata/logs/` (gitignored, `validation_latest.log` / `openwa_latest.log`)
- Script: `replay-test.ps1`
- Skill: `.claude/skills/replay-test/SKILL.md`

Without `OPENWA_REPLAY_TEST`, the validator runs in interactive mode with hotkeys (F9=team blocks, F10=landscape) and timed dumps.

## Crate Architecture

- **`openwa-types`** — Enums, structs, addresses, parsers (no_std compatible). The source of truth for all reverse-engineered type layouts and known addresses (`address.rs`). No game dependency.
- **`openwa-validator`** — WormKit cdylib that validates openwa-types against live WA.exe memory. Logs to `OpenWA_validation.log`. Read-only — never modifies game state.
- **`openwa-wormkit`** — WormKit cdylib that replaces WA functions with Rust. Logs to `OpenWA.log`. Uses MinHook for inline hooking. This is where reimplemented game logic lives.
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

## Key Files

- `crates/openwa-types/src/address.rs` — All known WA.exe addresses with comments
- `crates/openwa-wormkit/src/replacements/` — Function replacements (one file per subsystem)
- `crates/openwa-wormkit/src/wa_call.rs` — Helpers for calling WA functions (thiscall, stdcall wrappers)
- `crates/openwa-wormkit/src/rebase.rs` — ASLR delta computation
- `docs/re-notes/` — Reverse engineering documentation (task hierarchy, memory map, frontend screens)
- `docs/plans/` — Design docs and implementation plans

## Ghidra MCP

A Ghidra MCP bridge is configured in `.mcp.json`. When using Ghidra tools:
- **Prefer batch tools** (`batch_create_labels`, `batch_rename_function_components`) — single-item tools have address parsing bugs.
- WA.exe is loaded at image base 0x400000 in Ghidra.

## Third-Party RE Sources

- `C:\koodia\worms-re\thirdparty\wkJellyWorm\` — Extensive RE data: task hierarchy, vtables, enums, weapon structs
- `C:\koodia\worms-re\thirdparty\WormKit\` — Modding framework, game state structures

## Design Conventions

- `Ptr32 = u32` for pointer fields (compiles on 64-bit host, correct sizes on 32-bit target)
- `#[repr(u32)]` enums with `TryFrom<u32>` for safe conversion from game memory
- Unknown struct fields as `_unknown_XX` padding arrays
- Fixed-point: `Fixed(i32)` newtype, 16.16 format (0x10000 = 1.0)
- Naked asm uses `naked_asm!` (Rust 1.79+ syntax), not `asm!`
