# openwa-dll

Injected DLL (`openwa.dll`). Contains thin hook installation shims that wire `openwa-game` game logic into WA.exe via MinHook. Game logic itself lives in `openwa-game` — this crate is purely the wiring layer.

See the root `CLAUDE.md` for project-wide rules: architecture, calling conventions, ASLR rebasing, FFI style, design conventions.

## Hooking Patterns

Hooks use the `minhook` crate. Four patterns:

1. **Passthrough hook** (logging only): Call original via trampoline, log result.
2. **Full replacement**: Reimplement the function in Rust.

For `__usercall` functions, use a naked trampoline to capture register params before calling the Rust impl. **ECX preservation**: The standard `reg = ecx` trampoline variants do NOT preserve ECX across the cdecl impl call. MSVC-generated callers often loop calling thiscall functions without re-setting ECX between iterations (relying on the original function preserving it). Use the `preserve_ecx` variant for thiscall hooks where callers may rely on ECX being preserved: `usercall_trampoline!(fn name; impl_fn = path; reg = ecx; stack_params = N; ret_bytes = "0xN"; preserve_ecx)`.

3. **Vtable method replacement**: Use `vtable_replace!` to patch vtable slots at runtime. Write the replacement as `unsafe extern "thiscall" fn`. Save the original via `[ORIG_STATIC]` syntax if you need to call through. See `replacements/entity/cloud.rs`.
4. **Trap hook** (`install_trap!`): For functions whose only caller is now ported Rust. Panics if called unexpectedly.

### Bridge function patterns

When calling unported WA functions from Rust, use naked asm bridges. **Always pass the runtime target address as a cdecl parameter** (e.g., `rb(va::FUNC_ADDR)` as the last arg). Do NOT use `sym` + `jmp [ptr]` indirection through static pointers — this causes crashes due to relocation/PIC issues on x86 DLLs.

When a register param (e.g., EAX) must be set for the target function, load the call target into a **different** register (EBX, etc.) before the `call`. Do not use EAX for the target if EAX is also a parameter.

## Hook Installation

`replacements/mod.rs` orchestrates all hook installation via `install_all()`:

1. **Infrastructure hooks** (always installed): `headless`, `file_isolation`, `frame_hook`, `trace_desync`
2. **Baseline mode** (`OPENWA_TRACE_BASELINE=1`): returns early after infrastructure, skipping all gameplay hooks. Used by trace-desync for a "nearly vanilla" reference run.
3. **Gameplay hooks** (normal mode): all remaining subsystems — display, game_session, frontend, scheme, config, weapon, team, render, sprite, sound, speech, world_init, replay, entity, weapon_release, etc.

All hooks use `queue_enable_hook` + single `apply_queued()` call for batched MinHook enables (avoids repeated MH_EnableHook overhead).

## Key Files

- `src/hook.rs` — MinHook helpers: `install()`, `install_trap!`, `usercall_trampoline!` macro, `queue_enable_hook` + `apply_queued()`
- `src/lib.rs` — DLL entry point (`DllMain`), startup checks, `install_all()` call
- `src/replacements/mod.rs` — `install_all()` orchestration, infrastructure vs gameplay split
- `src/replacements/*.rs` — Per-subsystem hook shims (one file per WA subsystem)
- `src/replacements/entity/` — Vtable method replacements (cloud, filter, ...)
- `src/replacements/frame_hook.rs` — TurnManager_ProcessFrame hook, debug_sync, watchpoint arming (always installed)
- `src/replacements/trace_desync.rs` — Per-frame checksum logging for desync bisection
- `src/debug_server.rs` — TCP debug server (port 19840)
- `src/debug_watchpoint.rs` — Hardware watchpoint instrumentation (DR0-DR3)
- `src/snapshot.rs` — Game state snapshot for diffing
