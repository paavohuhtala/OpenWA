# openwa-dll

Injected DLL (`openwa.dll`). Contains thin hook installation shims that wire `openwa-game` game logic into WA.exe via MinHook. Game logic itself lives in `openwa-game` — this crate is purely the wiring layer.

See the root `CLAUDE.md` for project-wide rules: architecture, calling conventions, ASLR rebasing, FFI style, design conventions, and the **[RE / porting workflow](../../CLAUDE.md#re--porting-workflow)** (start there when adding a new hook).

## Generated hooks (preferred)

Most hooks come from `hooks/<subsystem>.toml` joined against `re/**/*.toml` at build time by `openwa-re-codegen`. Each `[[hook]]` entry declares `wa_function` (matches an `re/` entry by name) and `rust_impl` (fully-qualified Rust path). The build emits, into `crate::generated::hooks`:

- `const _CHECK_<wa>: extern "<cc>" fn(args) -> ret = <rust_impl>;` — typed signature guard. Wrong arity / wrong type fails the build at the impl site.
- For `custom_storage = true` functions: `#[unsafe(naked)] extern "cdecl" fn tramp_<wa>()` that captures the WA register/stack args and forwards to the cdecl impl.
- `pub unsafe fn install_<wa>() -> Result<(), String>` — call from your subsystem's `install()`.

Optional `[[hook]]` fields:

- `save_original = true` → also emits `static ORIG_<wa>: AtomicU32` + `call_original_<wa>(...)` for passthrough-style hooks that need to call through to WA.
- `preserve_registers = ["ecx"]` or `"all"` → extra push/pop pairs around the cdecl call. Use when the WA caller relies on a register staying intact (e.g. thiscall callers looping without re-setting ECX between iterations).

**Impl signature rule:**

- `custom_storage = true` hooks → impl is `extern "cdecl"`, args ordered (register-storage in declaration order, then stack-storage by ascending offset). The naked trampoline does the convention bridging.
- Default-storage hooks → impl matches the WA convention directly (`extern "thiscall"`, `extern "stdcall"`, `extern "fastcall"`, `extern "cdecl"`).

After migrating a hook to codegen, **delete the matching `va::FOO` const** in `crates/openwa-game/src/address.rs` if nothing else references it — `openwa_game::generated::addresses::*` is now canonical.

## Hand-written hook patterns (fallback)

Reach for these only when the codegen shape doesn't fit (e.g. vtable slot replacements, traps, ad-hoc passthroughs that aren't worth codegen-modelling). Hooks use the `minhook` crate. Four patterns:

1. **Passthrough hook** (logging only): Call original via trampoline, log result.
2. **Full replacement**: Reimplement the function in Rust.

For `__usercall` functions the codegen path handles register/stack capture automatically — set `custom_storage = true` in `re/` and (if the WA caller relies on a register staying intact across the cdecl call) `preserve_registers = ["ecx"]` in the hook entry. Hand-written naked-asm trampolines are no longer needed for usercall capture; reach for one only if the shape doesn't fit the codegen at all.

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

- `src/hook.rs` — MinHook helpers: `install()`, `install_trap!`, `queue_enable_hook` + `apply_queued()`
- `src/lib.rs` — DLL entry point (`DllMain`), startup checks, `install_all()` call
- `src/replacements/mod.rs` — `install_all()` orchestration, infrastructure vs gameplay split
- `src/replacements/*.rs` — Per-subsystem hook shims (one file per WA subsystem)
- `src/replacements/entity/` — Vtable method replacements (cloud, filter, ...)
- `src/replacements/frame_hook.rs` — TurnManager_ProcessFrame hook, debug_sync, watchpoint arming (always installed)
- `src/replacements/trace_desync.rs` — Per-frame checksum logging for desync bisection
- `src/debug_server.rs` — TCP debug server (port 19840)
- `src/debug_watchpoint.rs` — Hardware watchpoint instrumentation (DR0-DR3)
- `src/snapshot.rs` — Game state snapshot for diffing
